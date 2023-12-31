use crate::merkletree::MerkleTree;
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

thread_local! {
    static MT: RefCell<MerkleTree> = RefCell::new(MerkleTree::default());
}

const DATA_DIR: &str = "./.server/";


fn save_file(filename: &str, data: &[u8]) -> Message {
    let mut file = File::create(Path::new(DATA_DIR).join(Path::new(filename))).unwrap();
    file.write_all(data).unwrap();

    println!("Saved file {}", filename);

    /* Update merkle tree */
    block_on(async {
        let merkle_tree = MerkleTree::from_folder(Path::new(DATA_DIR)).await;
        MT.with(|mt| mt.replace(merkle_tree.clone()));
    });

    Message::FileAck { filename: filename.to_string(), hash: blake3::hash(data).to_string() }
}

fn delete_file(filename: &str) -> Message {
    let res = block_on(async {
        tokio::fs::remove_file(Path::new(DATA_DIR).join(Path::new(filename))).await
    });
    match res {
        Ok(_) => {
            println!("Deleted file {}", filename);

            /* Update merkle tree */
            block_on(async {
                let merkle_tree = MerkleTree::from_folder(Path::new(DATA_DIR)).await;
                MT.with(|mt| mt.replace(merkle_tree.clone()));
            });

            Message::DeleteFileAck { filename: filename.to_string(), deleted: true }
        }
        Err(_) => {
            println!("Unable to remove file {}", filename);
            Message::DeleteFileAck { filename: filename.to_string(), deleted: false }
        }
    }
}

/**Read file stored on disk, get merkle proof and return Message::File */
fn read_file(filename: &str) -> Message {
    if let Ok(mut file) = File::open(Path::new(DATA_DIR).join(Path::new(filename))) {
        /* Read file from disk */
        let mut data = Vec::new();
        file.read_to_end(&mut data).unwrap();

        /* Generate merkle proof */
        let p = block_on(async {
            let merkle_tree = MT.with(|mt| mt.borrow().clone());
            
            let path = Path::new(DATA_DIR).join(Path::new(filename));
            let proof = merkle_tree.get_proof(path.as_path()).await;

            proof
        });
    
        /* Convert merkle proof from OSString to Vec<Vec<u8>> */
        let p = p.map(|p| 
            p.into_iter()
            .map(|s| s.into_string().unwrap_or_default().into_bytes())
            .collect::<Vec<Vec<u8>>>()
        );

        println!("Read file {}", filename);
        
        Message::File { filename: filename.to_string() , data: data, merkle_proof: p.unwrap() }
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
