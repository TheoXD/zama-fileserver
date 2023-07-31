//use chrono::prelude::*;
use serde::{Deserialize, Serialize};
use bromberg_sl2::{HashMatrix};

#[derive(PartialEq, Clone, Serialize, Deserialize, Debug)]
pub enum Message {
    //Echo { payload: String, ts: DateTime<Utc> },
    FileUpload { filename: String, data: Vec<u8> },
    FileAck { filename: String, hash: HashMatrix },
    FileRequest { filename: String },
    File { filename: String, data: Vec<u8>, proof: HashMatrix },
    FileNotFound { filename: String },
    DeleteFileRequest { filename: String },
    DeleteFileAck { filename: String, deleted: bool },
    
    Heartbeat,
    HeartbeatAck,
}
