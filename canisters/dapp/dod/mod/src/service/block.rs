use crate::memory::BLOCKS;
use crate::service::config::get_difficulty_adjust_epoch;
use dod_utils::types::{BlockData, Height};

pub fn get_last_block() -> Option<(u64, BlockData)> {
    BLOCKS.with_borrow(|b| b.last_key_value())
}

pub fn get_block_by_height(height: u64) -> Option<BlockData> {
    BLOCKS.with(|v| v.borrow().get(&height).map(|v| v.clone()))
}

pub fn get_blocks() -> Vec<BlockData> {
    BLOCKS.with(|v| {
        v.borrow()
            .iter()
            .map(|(_, v)| v.clone())
            .collect::<Vec<BlockData>>()
    })
}

pub fn get_blocks_range(from: Height, to: Height) -> Vec<BlockData> {
    BLOCKS.with(|v| {
        v.borrow()
            .range(from.clone()..=to.clone())
            .map(|(_, v)| v.clone())
            .collect::<Vec<BlockData>>()
    })
}

pub fn get_last_epoch_failed_blocks_count(start_height: Height) -> (u64, u64, f64) {
    let epoch = get_difficulty_adjust_epoch().unwrap_or(0);
    let from = if start_height.clone() < epoch {
        0u64
    } else {
        start_height.clone() - epoch
    };
    let times = BLOCKS.with(|v| {
        v.borrow()
            .range(from.clone()..=start_height.clone())
            .filter(|(_, v)| v.winner.is_none())
            .count()
    }) as u64;
    let range = start_height.clone() - from.clone();

    (times, range, (times / range) as f64)
}
