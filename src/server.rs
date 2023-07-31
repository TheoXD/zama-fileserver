use crate::protocol::Message;
use chrono::prelude::*;

use hydroflow::{hydroflow_syntax, futures};
use hydroflow::scheduled::graph::Hydroflow;
use hydroflow::util::{UdpSink, UdpStream};

use std::net::SocketAddr;

use std::fs::File;
use std::io::prelude::*;
use std::path::Path;

use futures::executor::block_on;

use std::cell::RefCell;

use bromberg_sl2::{HashMatrix, hash_par};
use crate::bromberg_hash::{get_folder_hash, matsub};

thread_local! {
    static ROOT: RefCell<HashMatrix> = RefCell::new(HashMatrix::default());
}

const DATA_DIR: &str = "./.server/";


fn save_file(filename: &str, data: &[u8]) -> Message {
    let mut file = File::create(Path::new(DATA_DIR).join(Path::new(filename))).unwrap();
    file.write_all(data).unwrap();

    println!("Saved file {}", filename);

    let folder_hash = block_on(async {
        get_folder_hash(Path::new(DATA_DIR)).await
    });
    //Update folder hash
    ROOT.with(|r| *r.borrow_mut() = folder_hash);

    Message::FileAck { filename: filename.to_string(), hash: hash_par(data) }
}

fn delete_file(filename: &str) -> Message {
    let res = block_on(async {
        tokio::fs::remove_file(Path::new(DATA_DIR).join(Path::new(filename))).await
    });
    match res {
        Ok(_) => {
            println!("Deleted file {}", filename);

            let folder_hash = block_on(async {
                get_folder_hash(Path::new(DATA_DIR)).await
            });
            //Update folder hash
            ROOT.with(|r| *r.borrow_mut() = folder_hash);

            Message::DeleteFileAck { filename: filename.to_string(), deleted: true }
        }
        Err(_) => {
            println!("Unable to remove file {}", filename);
            Message::DeleteFileAck { filename: filename.to_string(), deleted: false }
        }
    }
}

/**Read file stored on disk, generate merkle tree (TODO: make thread local) and return Message::File */
fn read_file(filename: &str) -> Message {
    if let Ok(mut file) = File::open(Path::new(DATA_DIR).join(Path::new(filename))) {
        /* Read file from disk */
        let mut data = Vec::new();
        file.read_to_end(&mut data).unwrap();
        let data_hash = hash_par(data.as_slice());

        //TODO: do substract
        let folder_hash = ROOT.with(|r| r.borrow().clone());
        let proof = matsub(folder_hash, data_hash);

        println!("Read file {}", filename);
        
        Message::File { filename: filename.to_string() , data: data, merkle_proof: proof }
    } else {
        Message::FileNotFound { filename: filename.to_string() }
    }
}

pub(crate) async fn run_server(outbound: UdpSink, inbound: UdpStream) {
    println!("Server live!");

    let mut flow: Hydroflow = hydroflow_syntax! {
        // Define shared inbound and outbound channels
        inbound_chan = source_stream_serde(inbound) -> map(|udp_msg| udp_msg.unwrap()) -> tee();
        outbound_chan = union() -> dest_sink_serde(outbound);

        // Print all messages for debugging purposes
        inbound_chan[1]
            -> for_each(|(m, a): (Message, SocketAddr)| println!("{}: Got {:?} from {:?}", Utc::now(), m, a));

        // Demux and destructure the inbound messages into separate streams
        inbound_demuxed = inbound_chan[0]
            ->  demux(|(msg, addr), var_args!(file_upload_ch, file_request_ch, del_file_request_ch, heartbeat_ch, errs_ch)|
                    match msg {
                        Message::FileUpload {filename, data} => file_upload_ch.give((filename, data, addr)),
                        Message::FileRequest {filename} => file_request_ch.give((filename, addr)),
                        Message::DeleteFileRequest {filename} => del_file_request_ch.give((filename, addr)),
                        Message::Heartbeat => heartbeat_ch.give(addr),
                        _ => errs_ch.give((msg, addr)),
                    }
                );

        inbound_demuxed[file_upload_ch]
            -> map(|(filename, data, addr)| (save_file(&filename, data.as_slice()), addr) )
            -> map(|(file_ack, addr)| (file_ack, addr) ) -> [0]outbound_chan;

        inbound_demuxed[del_file_request_ch]
            -> map(|(filename, addr)| (delete_file(&filename), addr) )
            -> map(|(del_file_ack, addr)| (del_file_ack, addr) ) -> [3]outbound_chan;

        inbound_demuxed[file_request_ch] -> map(|(filename, addr)| (read_file(&filename) , addr)) -> [1]outbound_chan;

        // Respond to Heartbeat messages
        inbound_demuxed[heartbeat_ch] -> map(|addr| (Message::HeartbeatAck, addr)) -> [2]outbound_chan;

        // Print unexpected messages
        inbound_demuxed[errs_ch]
            -> for_each(|(msg, addr)| println!("Received unexpected message type: {:?} from {:?}", msg, addr));

    };

    /* Create server data folder before running the flow */
    let _ = tokio::fs::create_dir_all(DATA_DIR).await;

    // run the server flow
    flow.run_async().await;
}
