use std::collections::HashMap;
use std::path::Path;
use std::ffi::OsString;


#[derive(Debug, Clone, Default)]
pub struct MTNode {
    pub hash: OsString,
    pub l: Option<OsString>,
    pub r: Option<OsString>,
    pub parent: Option<OsString>,
}


#[derive(Debug, Clone, Default)]
pub struct MerkleTree {
    pub root: Option<OsString>,
    pub data: HashMap<OsString, MTNode>,
}

impl MerkleTree {
    pub async fn from_folder(path: &Path) -> MerkleTree {
        let mut leaves = Vec::new();
        let dir = tokio::fs::read_dir(path).await;
        match dir {
            Ok(mut rd) => {
                while let Some(child) = rd.next_entry().await.unwrap() {
                    if !child.metadata().await.unwrap().is_dir() {
                        let _ = tokio::fs::read(child.path()).await.map(|data| {
                            leaves.push(MTNode {
                                hash: OsString::from(
                                    blake3::hash(data.as_slice()).to_string()
                                ),
                                ..MTNode::default()
                            });
                        });
                    }
                }
            }
            Err(e) => println!("Error: {}", e),
        }
        Self::from(leaves)
    }

    /* Construct a merkle tree from a set of leaf nodes */
    pub fn from(leaves: Vec<MTNode>) -> MerkleTree {
        if leaves.len() == 0 {
            return MerkleTree::default();
        }
        
        let mut hashmap: HashMap<OsString, MTNode> = HashMap::new();

        /* Iterate over vector of nodes, updating it after each iteration until we have only one node left */
        /* that should be the root node */
        let mut nodes = leaves;
        while nodes.len() > 1 {
            nodes = nodes.chunks_mut(2).flat_map(|pair| {
                let mut pair = pair.into_iter();

                /* The first value in the pair will always exist, we can unwrap safely */
                /* The second value however is not guaranteed */
                let left = pair.next().unwrap();
                if let Some(right) = pair.next() {
                    let l = left.hash.as_os_str_bytes();

                    /* Make sure the second vector has same length of 64 bytes */
                    let mut rv = right.hash.as_os_str_bytes().to_vec();
                    rv.resize(64, 0);
                    let r = &rv[..];

                    let parent = MTNode {
                        hash: OsString::from(
                            blake3::hash([

                                /* XOR both hashes into one to get the parent hash */
                                l.iter().zip(r.iter())
                                .map(|(&x1, &x2)| x1 ^ x2)
                                .collect::<Vec<u8>>()
                                .as_slice()

                            ].concat().as_slice()).to_string()
                        ),
                        l: Some(left.hash.clone()),
                        r: Some(right.hash.clone()),
                        ..MTNode::default()
                    };

                    left.parent = Some(parent.hash.clone());
                    right.parent = Some(parent.hash.clone());

                    hashmap.insert(left.hash.clone(), left.clone());
                    hashmap.insert(right.hash.clone(), right.clone());

                    vec![parent]
                } else {
                    let parent = MTNode {
                        hash: OsString::from(
                            blake3::hash(
                                /* Here we don't need to XOR and just hash the first value */
                                left.hash.as_os_str_bytes()
                            ).to_string()
                        ),
                        l: Some(left.hash.clone()),
                        ..MTNode::default()
                    };
                    left.parent = Some(parent.hash.clone());
                    hashmap.insert(left.hash.clone(), left.clone());

                    vec![parent]
                }
            }).collect::<Vec<MTNode>>();
            
            //println!("Added nodes: {:?}", nodes);

            hashmap.extend(
                nodes.iter()
                .map(|leaf| (leaf.hash.clone(), leaf.clone()))
                .collect::<HashMap<OsString, MTNode>>()
            );
        }

        let root = nodes.pop().unwrap();

        /* Add root node to hashmap as well */
        hashmap.insert(root.hash.clone(), root.clone());

        MerkleTree {
            root: Some(root.hash),
            data: hashmap,
        }
    }

    /* Get merkle proof from file path */
    pub async fn get_proof(&self, path: &Path) -> Option<Vec<OsString>> {
        /* Read data from file and hash it */
        let mut data = Vec::new();
        let _ = tokio::fs::read(path).await.map(|d| data = d);
        let file_hash = blake3::hash(data.as_slice()).to_string();

        /* Create proof by going up the chain */
        let mut proof = Vec::new();
        let mut node = self.data.get(&OsString::from(file_hash)).unwrap();
        while let Some(parent) = node.parent.clone() {
            /* Find whether node is on the left or right of parent */
            if let Some(parent_node) = self.data.get(&parent) {
                if parent_node.l.clone().unwrap() == node.hash {
                    //And add it's opposite sibling to the proof
                    proof.push(parent_node.r.clone().unwrap_or_default());
                } else {
                    //And add it's opposite sibling to the proof
                    proof.push(parent_node.l.clone().unwrap());
                }
                node = parent_node;
            } else {
                break;
            }
        }

        Some(proof)
    }

    pub async fn verify_data_with_proof(data: &Vec<u8>, proof: Vec<OsString>, root: OsString) -> bool {
        let file_hash = blake3::hash(data.as_slice()).to_string();

        /* Iterate over proof, folding into the final root hash */
        let root_hash = proof.iter().fold(OsString::from(file_hash), |acc_hash, h| {
            /* Make sure the proof hashes have the same length of 64 bytes if empty */
            let mut hv = h.as_os_str_bytes().to_vec();
            hv.resize(64, 0);
            let other = &hv[..];

            /* XOR both hashes into one to get the parent hash */
            let combined_hash = acc_hash.as_os_str_bytes().iter()
            .zip(other.iter())
            .map(|(&x1, &x2)| x1 ^ x2)
            .collect::<Vec<u8>>();

            let parent_hash = OsString::from(blake3::hash(combined_hash.as_slice()).to_string());
            parent_hash
        });

        root_hash == root
    }
    /* Unused 
    pub async fn verify_file_with_proof(path: &Path, proof: Vec<OsString>, root: OsString) -> bool {
        /* Read file contents and call the function above supplying data to it */
        let mut data = Vec::new();
        let _ = tokio::fs::read(path).await.map(|d| data = d);
        MerkleTree::verify_data_with_proof(&data, proof, root).await
    }
    */
}