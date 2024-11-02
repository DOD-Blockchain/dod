use candid::{CandidType, Deserialize};
use serde::Serialize;
use std::cmp::Ordering;

#[derive(Debug, Eq, Clone, CandidType, Serialize, Deserialize)]
pub struct Bitwork {
    pub pre: u64,
    pub post_hex: String,
}

impl PartialEq<Self> for Bitwork {
    fn eq(&self, other: &Self) -> bool {
        self.pre == other.pre && self.post_hex == other.post_hex
    }
}

impl PartialOrd<Self> for Bitwork {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Bitwork {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (self.pre * 16 + u64::from_str_radix(self.post_hex.as_str(), 16).unwrap())
            .cmp(&(other.pre * 16 + u64::from_str_radix(other.post_hex.as_str(), 16).unwrap()))
    }
}

impl Bitwork {
    #[allow(dead_code)]
    fn validate(&self) -> Result<(), String> {
        if self.pre > 64 {
            return Err("Invalid bitwork".to_string());
        }

        if self.post_hex.len() > 1 {
            return Err("Invalid bitwork".to_string());
        }

        if self.pre == 64 && self.post_hex != "0" {
            return Err("Invalid bitwork".to_string());
        }

        let mut err = None;
        self.post_hex.chars().for_each(|c| {
            if c.to_digit(16).is_none() {
                err = Some(Err("Invalid bitwork".to_string()));
            }
        });
        if err.is_some() {
            err.unwrap()
        } else {
            Ok(())
        }
    }
    #[allow(dead_code)]
    fn to_string(&self) -> Result<String, String> {
        match self.validate() {
            Ok(_) => Ok(format!("{}.{}", self.pre, self.post_hex)),
            Err(e) => Err(e),
        }
    }
    #[allow(dead_code)]
    fn from_str(s: &str) -> Result<Self, String> {
        let (pre, post_hex) = s.split_once('.').unwrap();
        let res = pre.parse::<u64>();
        if res.is_err() {
            return Err("Invalid bitwork".to_string());
        }
        if res.clone().unwrap() > 64 {
            return Err("Invalid bitwork".to_string());
        }
        if post_hex.len() > 1 {
            return Err("Invalid bitwork".to_string());
        }
        if res.clone().unwrap() == 64 && post_hex != "0" {
            return Err("Invalid bitwork".to_string());
        }
        let mut err = None;
        post_hex.chars().for_each(|c| {
            if c.to_digit(16).is_none() {
                err = Some(Err("Invalid bitwork".to_string()));
            }
        });

        if err.is_some() {
            err.unwrap()
        } else {
            Ok(Bitwork {
                pre: pre.parse::<u64>().unwrap(),
                post_hex: post_hex.to_string(),
            })
        }
    }
}

pub fn bitwork_from_height(block_height: u64, difficulty_epoch: u64) -> Result<Bitwork, String> {
    if difficulty_epoch == 0 {
        return Err("Invalid difficulty epoch".to_string());
    }
    let diff = block_height / difficulty_epoch;
    let pre = format!("{}", diff / 16);
    let mut post_hex = format!("{:x}", (diff % 16) as u8);
    let mut _pre = pre.parse::<u64>().unwrap();
    if pre.parse::<u64>().unwrap() > 64 {
        _pre = 64;
    }
    if _pre == 64 && post_hex != "0" {
        post_hex = "0".to_string();
    }
    Ok(Bitwork {
        pre: _pre,
        post_hex,
    })
}

pub fn bitwork_plus_bit_hex(bitwork: Bitwork, num: u8) -> Result<Bitwork, String> {
    if bitwork.pre == 64 {
        return Ok(Bitwork {
            pre: 64,
            post_hex: "0".to_string(),
        });
    }
    let mut pre = bitwork.pre;
    let mut post_hex = bitwork.post_hex.clone();

    post_hex = match u8::from_str_radix(post_hex.as_str(), 16) {
        Ok(e) => {
            if e == (16 - num) {
                pre += 1;
                "0".to_string()
            } else {
                format!("{:x}", e + num)
            }
        }
        Err(_) => return Err("Invalid bitwork".to_string()),
    };
    if pre > 64 {
        pre = 64
    };
    Ok(Bitwork { pre, post_hex })
}

pub fn bitwork_minus_bit_hex(bitwork: Bitwork, num: u8) -> Result<Bitwork, String> {
    if bitwork.pre == 0 && bitwork.post_hex == "0" {
        return Ok(bitwork.clone());
    }
    let mut pre = bitwork.pre;
    let mut post_hex = bitwork.post_hex.clone();

    post_hex = match u8::from_str_radix(post_hex.as_str(), 16) {
        Ok(e) => {
            if e == (16 - num) {
                if pre > 0 {
                    pre -= 1;
                }
                "f".to_string()
            } else {
                if e >= num {
                    format!("{:x}", e - num)
                } else {
                    if pre > 0 {
                        pre -= 1;
                        format!("{:x}", e + 16 - num)
                    } else {
                        format!("{:x}", 0)
                    }
                }
            }
        }
        Err(_) => return Err("Invalid bitwork".to_string()),
    };
    #[allow(unused_comparisons)]
    if pre < 0 {
        pre = 0
    };
    Ok(Bitwork { pre, post_hex })
}

pub fn bitwork_plus_one_hex(bitwork: Bitwork) -> Result<Bitwork, String> {
    if bitwork.pre == 64 {
        return Ok(Bitwork {
            pre: 64,
            post_hex: "0".to_string(),
        });
    }
    let mut pre = bitwork.pre;
    let mut post_hex = bitwork.post_hex.clone();

    post_hex = match u8::from_str_radix(post_hex.as_str(), 16) {
        Ok(e) => {
            if e == 15 {
                pre += 1;
                "0".to_string()
            } else {
                format!("{:x}", e + 1)
            }
        }
        Err(_) => return Err("Invalid bitwork".to_string()),
    };
    if pre > 64 {
        pre = 64
    };
    Ok(Bitwork { pre, post_hex })
}

pub fn bitwork_minus_one_hex(bitwork: Bitwork) -> Result<Bitwork, String> {
    if bitwork.pre == 0 && bitwork.post_hex == "0" {
        return Ok(bitwork.clone());
    }
    let mut pre = bitwork.pre;
    let mut post_hex = bitwork.post_hex.clone();

    post_hex = match u8::from_str_radix(post_hex.as_str(), 16) {
        Ok(e) => {
            if e == 0 {
                pre -= 1;
                "f".to_string()
            } else {
                format!("{:x}", e - 1)
            }
        }
        Err(_) => return Err("Invalid bitwork".to_string()),
    };
    #[allow(unused_comparisons)]
    if pre < 0 {
        pre = 0
    };
    Ok(Bitwork { pre, post_hex })
}

pub fn bitwork_match_hash(
    current_hash: String,
    target_hash: String,
    bitwork: Bitwork,
    reverse: bool,
) -> Result<bool, String> {
    let mut target =
        hex::decode(target_hash.as_str()).map_err(|_| "Invalid target hash".to_string())?;

    if target.len() != 32 {
        return Err("Invalid target hash width".to_string());
    }
    if reverse {
        target.reverse();
    }

    let target_string = hex::encode(target);
    let binding1 = current_hash.clone();
    let current_pre = binding1.get(..bitwork.pre as usize);

    if current_pre.is_none() {
        return Err("Invalid bitwork".to_string());
    }
    let binding2 = current_hash.clone();

    let current_post = binding2.get(bitwork.pre as usize..bitwork.pre as usize + 1);
    if current_post.is_none() {
        return Err("Invalid bitwork".to_string());
    }

    let binding3 = target_string.clone();
    let target_pre = binding3.get(..bitwork.pre as usize);
    if target_pre.is_none() {
        return Err("Invalid bitwork".to_string());
    }

    let binding4 = target_string.clone();

    let target_post = binding4.get(bitwork.pre as usize..bitwork.pre as usize + 1);
    if target_post.is_none() {
        return Err("Invalid bitwork".to_string());
    }

    ic_cdk::println!(
        "current_pre: {:?}, target_pre: {:?}, current_post: {:?}, target_post: {:?}",
        current_pre,
        target_pre,
        current_post,
        target_post
    );

    if current_pre.unwrap() == target_pre.unwrap()
        && u32::from_str_radix(current_post.unwrap(), 16).unwrap()
            >= u32::from_str_radix(bitwork.post_hex.as_str(), 16).unwrap()
    {
        Ok(true)
    } else {
        Ok(false)
    }
}

pub fn merge_bitwork(bitwork_height: Bitwork, bitwork_tx: Bitwork) -> Bitwork {
    let mut pre = bitwork_height.pre + bitwork_tx.pre;
    let post = u32::from_str_radix(bitwork_height.post_hex.as_str(), 16).unwrap()
        + u32::from_str_radix(bitwork_tx.post_hex.as_str(), 16).unwrap();
    let _post = post / 16;
    let mut post = format!("{:x}", (post % 16) as u8);
    pre += _post as u64;
    if pre > 64 {
        pre = 64;
    }
    if pre == 64 {
        post = "0".to_string();
    }

    Bitwork {
        pre,
        post_hex: post,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bitwork_plus_bit_hex() {
        let bitwork = Bitwork {
            pre: 3,
            post_hex: "3".to_string(),
        };
        let result = bitwork_plus_bit_hex(bitwork, 4);
        assert_eq!(
            result,
            Ok(Bitwork {
                pre: 3,
                post_hex: "7".to_string()
            })
        );

        let bitwork2 = Bitwork {
            pre: 3,
            post_hex: "c".to_string(),
        };
        let result = bitwork_plus_bit_hex(bitwork2, 4);
        assert_eq!(
            result,
            Ok(Bitwork {
                pre: 4,
                post_hex: "0".to_string()
            })
        );
    }
    #[test]
    fn test_bitwork_minus_bit_hex() {
        let bitwork = Bitwork {
            pre: 0,
            post_hex: "3".to_string(),
        };
        let result = bitwork_minus_bit_hex(bitwork, 4);
        assert_eq!(
            result,
            Ok(Bitwork {
                pre: 0,
                post_hex: "0".to_string()
            })
        );

        let bitwork = Bitwork {
            pre: 1,
            post_hex: "4".to_string(),
        };
        let result = bitwork_minus_bit_hex(bitwork, 4);
        assert_eq!(
            result,
            Ok(Bitwork {
                pre: 1,
                post_hex: "0".to_string()
            })
        );

        let bitwork = Bitwork {
            pre: 1,
            post_hex: "3".to_string(),
        };
        let result = bitwork_minus_bit_hex(bitwork, 4);
        assert_eq!(
            result,
            Ok(Bitwork {
                pre: 0,
                post_hex: "f".to_string()
            })
        );
    }
}
