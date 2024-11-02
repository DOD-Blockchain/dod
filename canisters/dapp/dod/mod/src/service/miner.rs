use crate::memory::{BLOCKS, CANDIDATES, MINERS, SIGS};
use crate::service::block::get_last_block;
use crate::verifier::{check_signed_reveal_psbt, checked_signed_commit_psbt_b64};
use candid::Principal;
use dod_utils::bitwork::bitwork_match_hash;
use dod_utils::types::{
    BlockRange, BlockSigs, BtcAddress, Height, MinerBlockData, MinerCandidate, MinerInfo,
    MinerStatus, MinerSubmitResponse, MinterCandidates,
};
use std::collections::BTreeMap;

pub fn register_miner(
    owner: Principal,
    btc_address: String,
    ecdsa_pubkey: Vec<u8>,
) -> Result<MinerInfo, String> {
    match check_miner_if_existed(owner) {
        None => {
            let miner_info = MinerInfo {
                owner,
                status: MinerStatus::Activate,
                ecdsa_pubkey,
                btc_address: btc_address.clone(),
                reward_cycles: None,
                claimed_dod: 0,
                total_dod: 0,
            };

            MINERS.with(|v| {
                v.borrow_mut()
                    .insert(BtcAddress(btc_address.clone()), miner_info.clone())
            });

            Ok(miner_info)
        }
        Some(_) => Err("Miner already existed".to_string()),
    }
}

pub fn get_current_miners_length() -> u32 {
    MINERS.with(|v| {
        let miners = v.borrow();
        miners.len() as u32
    })
}
pub fn check_miner_if_existed(caller: Principal) -> Option<MinerInfo> {
    MINERS.with(|v| {
        let miners = v.borrow();
        miners
            .iter()
            .find(|(_, v)| v.owner == caller)
            .map(|v| v.1.clone())
    })
}

pub fn add_block_candidate(height: Height, miner_candidate: MinerCandidate) {
    let s = CANDIDATES.with(|v| v.borrow().get(&height));
    match s {
        None => {
            let mut _v = MinterCandidates {
                candidates: BTreeMap::new(),
            };
            _v.candidates
                .insert(miner_candidate.btc_address.clone(), miner_candidate.clone());
            CANDIDATES.with(|v| v.borrow_mut().insert(height, _v));
        }
        Some(r) => {
            let mut _v = r.clone();
            _v.candidates
                .insert(miner_candidate.btc_address.clone(), miner_candidate.clone());
            CANDIDATES.with(|v| v.borrow_mut().insert(height, _v));
        }
    }
}

pub fn get_block_candidates(height: Height) -> Vec<MinerCandidate> {
    CANDIDATES.with(|v| {
        let v = v.borrow();
        v.get(&height)
            .map(|v| v.clone())
            .unwrap_or(MinterCandidates {
                candidates: BTreeMap::new(),
            })
            .candidates
            .iter()
            .map(|v| v.1.clone())
            .collect::<Vec<MinerCandidate>>()
    })
}

pub fn get_mining_history_for_miners(
    btc_address: String,
    block_range: BlockRange,
) -> Vec<MinerBlockData> {
    CANDIDATES.with_borrow(|v| {
        v.range(block_range.0..block_range.1)
            .filter(|f| f.1.candidates.get(&btc_address).is_some())
            .map(|(b, v)| {
                let block = BLOCKS.with_borrow(|bc| bc.get(&b).unwrap());
                let difficulty = block.difficulty;
                let winner = block.winner;
                let res = v
                    .candidates
                    .get(&btc_address)
                    .map(|c| MinerBlockData {
                        block_height: b,
                        winner: winner.is_some() && winner.unwrap().btc_address == btc_address,
                        cycles_price: c.cycles_price,
                        submit_time: c.submit_time,
                        difficulty,
                    })
                    .unwrap();
                res
            })
            .collect::<Vec<MinerBlockData>>()
    })
}

pub fn check_if_in_candidate(btc_address: String, block: Height) -> Option<MinerCandidate> {
    CANDIDATES.with(|v| {
        let v = v.borrow();
        v.get(&block)
            .unwrap_or(MinterCandidates {
                candidates: BTreeMap::new(),
            })
            .candidates
            .get(&btc_address)
            .map(|v| v.clone())
    })
}

pub fn get_miner_by_address(address: String) -> Option<MinerInfo> {
    MINERS.with(|v| {
        let miners = v.borrow();
        miners.get(&BtcAddress(address.clone()))
    })
}

pub fn get_miner_by_principal(principal: Principal) -> Option<MinerInfo> {
    MINERS.with(|v| {
        let miners = v.borrow();
        miners
            .iter()
            .find(|(_, v)| v.owner == principal)
            .map(|v| v.1.clone())
    })
}

pub fn miner_submit_hashes(
    caller: Principal,
    btc_address: String,
    signed_commit_psbt: String,
    signed_reveal_psbt: String,
    cycles_price: u128,
) -> Result<MinerSubmitResponse, String> {
    match check_miner_if_existed(caller) {
        Some(miner) => {
            let block = get_last_block().unwrap().1;

            if block.winner.is_some() {
                ic_cdk::println!("Block already mined {:?}", block.winner);
                return Err("Block already mined".to_string());
            }

            if block.next_block_time < ic_cdk::api::time() {
                return Err("Not time to submit hash".to_string());
            }

            if check_if_in_candidate(btc_address.clone(), block.height.clone()).is_some() {
                return Err("Miner already submitted hash".to_string());
            }

            let mut rev = block.hash.clone();
            rev.reverse();

            let (commit_txid, script_buf) = checked_signed_commit_psbt_b64(
                signed_commit_psbt.as_str(),
                miner.ecdsa_pubkey.clone(),
                rev,
            )?;

            check_signed_reveal_psbt(
                signed_reveal_psbt.as_str(),
                script_buf,
                miner.ecdsa_pubkey.clone(),
                commit_txid.clone(),
                miner.btc_address.clone(),
            )?;

            let block_hash = hex::encode(block.hash.clone());
            let result = bitwork_match_hash(
                commit_txid.clone(),
                block_hash,
                block.difficulty.clone(),
                false,
            )?;

            if result == false {
                ic_cdk::println!("bitwork_match_hash  result is {:?}", result);
                Err("Bitwork match failed".to_string())
            } else {
                // write candidate queue
                add_block_candidate(
                    block.height.clone(),
                    MinerCandidate {
                        btc_address: btc_address.clone(),
                        cycles_price: cycles_price.clone(),
                        signed_commit_psbt,
                        submit_time: ic_cdk::api::time(),
                        signed_reveal_psbt,
                    },
                );

                Ok(MinerSubmitResponse {
                    block_height: block.height.clone(),
                    cycles_price: cycles_price.clone(),
                })
            }
        }
        None => Err("Miner not found".to_string()),
    }
}

pub fn load_sigs_by_height(height: Height) -> Option<BlockSigs> {
    SIGS.with(|v| {
        let sigs = v.borrow();
        sigs.get(&height).map(|v| v.clone())
    })
}
