use crate::protocol::Message;
use crate::Opts;
use blake3::Hash;
use chrono::prelude::*;
use tokio::io::{AsyncWriteExt, AsyncReadExt};
use std::net::SocketAddr;
use std::path::Path;
use std::ffi::OsString;

use hydroflow::hydroflow_syntax;
use hydroflow::util::{UdpSink, UdpStream};
use hydroflow::futures;
use futures::executor::block_on;

use std::cell::RefCell;

use bromberg_sl2::{HashMatrix, hash_par};
use crate::bromberg_hash::{matadd};

thread_local! {
    static ROOT: RefCell<HashMatrix> = RefCell::new(HashMatrix::default());
}

const DATA_DIR: &str = "./.client/";
const N_OF_FILES: usize = 5;



async fn save_file(filename: String, data: Vec<u8>, proof: HashMatrix) -> String {
    let root = ROOT.with(|r| r.borrow().clone());

    let is_proof_valid = matadd(proof, hash_par(&data[..])) == root;
    println!("Is proof for file {} valid: {}", filename, is_proof_valid);

    if is_proof_valid {
        if let Ok(mut file) = tokio::fs::File::create(format!("./.client/{}", filename.clone())).await {
            let _ = file.write_all(data.as_slice()).await;
            format!("Saved file {}", filename)
        } else {
            format!("Unable to save file {}", filename)
        }
    } else {
        format!("Proof for file {} is invalid", filename)
    }
}

pub(crate) async fn run_client(outbound: UdpSink, inbound: UdpStream, opts: Opts) {
    // server_addr is required for client
    let server_addr = match opts.server_addr {
        Some(addr) => {
            println!("Connecting to server at {:?}", addr);
            addr
        }
        None => panic!("Client requires a server address"),
    };

    println!("Client live!");


    let (input, recv) = hydroflow::util::unbounded_channel::<Message>();

    let mut flow = hydroflow_syntax! {
        // Define shared inbound and outbound channels
        inbound_chan = source_stream_serde(inbound) -> map(|udp_msg| udp_msg.unwrap()) -> tee() ;
        outbound_chan = dest_sink_serde(outbound);

        // Write all received messages for debugging purposes to the .log file
        inbound_chan[1]
            -> map(|(m, a): (Message, SocketAddr)| format!("{}: Got {:?} from {:?}", Utc::now(), m, a))
            -> dest_file("client.log", true);

        inbound_demuxed = inbound_chan[0]
            ->  demux(|(msg, addr), var_args!(file_save_ch, errs_ch)|
                    match msg {
                        Message::FileAck {filename, hash} => println!("Upload of file {} with hash {} was successful!", filename, hash.to_hex()),
                        Message::File {filename, data, merkle_proof} => file_save_ch.give((filename, data, merkle_proof, addr)),
                        Message::DeleteFileAck {filename, deleted} => println!("File {} removed from server: {}", filename, deleted),
                        _ => errs_ch.give((msg, addr)),
                    }
                );

        /* When we receive a message to file_save_ch containing file data we save the file locally */
        /* save_file() does verification of the data and the proof against the root hash stored in memory */
        inbound_demuxed[file_save_ch]
                -> map(|(filename, data, merkleproof, _addr)| {
                    block_on(async {
                        save_file(filename, data, merkleproof).await
                    })
                } )
                -> dest_file("client.log", true);

        // Print unexpected messages
        inbound_demuxed[errs_ch]
            -> for_each(|(msg, addr)| println!("Received unexpected message type: {:?} from {:?}", msg, addr));

        /* Send directly to the server */
        source_stream(recv) 
            -> map(|l| (l, server_addr) )
            -> outbound_chan;
    };

    /* Step 1: Generate test files */
    let mut filenames = vec![];

    for i in 1..=N_OF_FILES {
        filenames.push(format!("file{}.txt", i));
    }

    /* Step 2: Generate test files */
    let _ = tokio::fs::create_dir_all(DATA_DIR).await;
    filenames.iter().for_each(|filename| {
        block_on(async {
            if let Ok(mut file) = tokio::fs::File::create(format!("./.client/{}", filename)).await {
                let _ = file.write_all(format!("Hello, world! This is {}", filename).as_bytes()).await;
                println!("Created ./files/{}", filename);
            }
        });
    });

    /* Step 3: Create root hash */
    let root_hash = filenames.iter().map(|filename| {
        block_on(async {
            if let Ok(mut file) = tokio::fs::File::open(format!("./.client/{}", filename)).await {
                let mut data = Vec::new();
                let _ = file.read_to_end(&mut data).await;
                hash_par(&data[..])
            } else {
                HashMatrix::default()
            }
        })
    }).fold(HashMatrix::default(), |acc, hash| {
        matadd(acc, hash)
    });
    println!("Created root hash: {:?} ", root_hash.to_hex());
    
    /* Step 4: Update ROOT hash and store it for the duration of the program */
    ROOT.with(|r| r.replace(root_hash.clone()));
    println!("Updated root hash");

    /* Step 5: Upload files to the server */
    filenames.iter().for_each(|filename| {
        block_on(async {
            if let Ok(mut file) = tokio::fs::File::open(format!("./.client/{}", filename)).await {
                let mut data = Vec::new();
                let _ = file.read_to_end(&mut data).await;
                let _ = input.send(Message::FileUpload { filename: filename.clone(), data: data });
            }
       });
    });

    println!("Running available");
    flow.run_available_async().await;

    /* Step 6: Delete local test files */
    filenames.iter().for_each(|filename| {
        block_on(async {
            let _ = tokio::fs::remove_file(format!("./.client/{}", filename)).await;
            println!("Deleted ./.client/{}", filename);
        });
    });

    /* Step 7: Request files back from the server, verifying their integrity */
    filenames.iter().for_each(|filename| {
        block_on(async {
            let _ = input.send(Message::FileRequest { filename: filename.clone() });
        });
    });

    flow.run_available_async().await;

    /* Step 8: Delete files from the server */
    filenames.iter().for_each(|filename| {
        block_on(async {
            let _ = input.send(Message::DeleteFileRequest { filename: filename.clone() });
        });
    });

    /* Step 9: Run the flow until termination */
    flow.run_async().await;
}
