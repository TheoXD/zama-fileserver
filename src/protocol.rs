//use chrono::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(PartialEq, Clone, Serialize, Deserialize, Debug)]
pub enum Message {
    //Echo { payload: String, ts: DateTime<Utc> },
    FileUpload { filename: String, data: Vec<u8> },
    FileAck { filename: String, hash: String },
    FileRequest { filename: String },
    File { filename: String, data: Vec<u8>, merkle_proof: Vec<Vec<u8>> },
    FileNotFound { filename: String },
    DeleteFileRequest { filename: String },
    DeleteFileAck { filename: String, deleted: bool },
    
    Heartbeat,
    HeartbeatAck,
}
