use bromberg_sl2::*;
use serde::{Serialize, Deserialize};
use core::mem::{transmute};
use std::path::Path;

#[derive(Default, Copy, Clone, PartialEq, Eq, Debug, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(remote = "HashMatrix")]
pub struct HashMatrixEx(
    #[serde(getter = "HashMatrix::get0")]
    pub u128,
    #[serde(getter = "HashMatrix::get1")]
    pub u128,
    #[serde(getter = "HashMatrix::get2")]
    pub u128,
    #[serde(getter = "HashMatrix::get3")]
    pub u128);

// Provide a conversion to construct the remote type.
impl From<HashMatrixEx> for HashMatrix {
    fn from(def: HashMatrixEx) -> HashMatrix {
        unsafe {
            transmute(def)
        }
    }
}

trait HashMatrixGetters {
    fn get0(&self) -> u128;
    fn get1(&self) -> u128;
    fn get2(&self) -> u128;
    fn get3(&self) -> u128;
}

impl HashMatrixGetters for HashMatrix {
    fn get0(&self) -> u128 {
        let a: HashMatrixEx = unsafe {
            transmute(*self)
        };
        a.0
    }

    fn get1(&self) -> u128 {
        let a: HashMatrixEx = unsafe {
            transmute(*self)
        };
        a.1
    }

    fn get2(&self) -> u128 {
        let a: HashMatrixEx = unsafe {
            transmute(*self)
        };
        a.2
    }

    fn get3(&self) -> u128 {
        let a: HashMatrixEx = unsafe {
            transmute(*self)
        };
        a.3
    }
}

#[derive(Debug)]
pub struct U256(u128, u128);

#[inline]
pub fn matadd(a: HashMatrix, b: HashMatrix) -> HashMatrix {
    let ex: HashMatrixEx = unsafe {
        transmute(a)
    };
    let bb: HashMatrixEx = unsafe {
        transmute(b)
    };
    let hmat = HashMatrixEx(
        add(U256(ex.0, 0), U256(bb.0, 0)).0,
        add(U256(ex.1, 0), U256(bb.1, 0)).0,
        add(U256(ex.2, 0), U256(bb.2, 0)).0,
        add(U256(ex.3, 0), U256(bb.3, 0)).0,
    );
    let mat_exposed: HashMatrix = unsafe {
        transmute(hmat)
    };
    mat_exposed
}

#[inline]
pub const fn add(x: U256, y: U256) -> U256 {
    let (low, carry) = x.1.overflowing_add(y.1);

    let (high, _) = x.0.overflowing_add(y.0).0.overflowing_add(carry as u128);
    U256(high, low)
}

#[inline]
pub fn matsub(a: HashMatrix, b: HashMatrix) -> HashMatrix {
    let ex: HashMatrixEx = unsafe {
        transmute(a)
    };
    let bb: HashMatrixEx = unsafe {
        transmute(b)
    };
    let hmat = HashMatrixEx(
        substract2(U256(ex.0, 0), U256(bb.0, 0)).0,
        substract2(U256(ex.1, 0), U256(bb.1, 0)).0,
        substract2(U256(ex.2, 0), U256(bb.2, 0)).0,
        substract2(U256(ex.3, 0), U256(bb.3, 0)).0,
    );
    let mat_exposed: HashMatrix = unsafe {
        transmute(hmat)
    };
    mat_exposed
}

#[inline]
pub const fn substract2(x: U256, y: U256) -> U256 {
    let (low, _carry) = (x.1).overflowing_sub(y.1);
    let high = x.0.wrapping_sub(_carry as u128).wrapping_sub(y.0);
    U256(high, low)
}


pub async fn get_folder_hash(path: &Path) -> HashMatrix {
    let mut leaves: Vec<HashMatrix> = Vec::new();
    let dir = tokio::fs::read_dir(path).await;
    match dir {
        Ok(mut rd) => {
            while let Some(child) = rd.next_entry().await.unwrap() {
                if !child.metadata().await.unwrap().is_dir() {
                    let _ = tokio::fs::read(child.path()).await.map(|data| {
                        leaves.push(hash_par(data.as_slice()));
                    });
                }
            }
        }
        Err(e) => println!("Error: {}", e),
    }
    leaves.iter().fold(HashMatrix::default(), |acc, x| matadd(acc, *x))
}