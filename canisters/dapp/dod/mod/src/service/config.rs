use crate::memory::CONFIG;
use crate::protocol::vec_to_u832;
use candid::Principal;
use dod_utils::bitwork::Bitwork;
use dod_utils::types::{HalvingSettings, Height};

pub fn get_token_canister() -> Result<Principal, String> {
    CONFIG.with(|config| {
        config
            .borrow()
            .dod_service
            .as_ref()
            .and_then(|dod_service| dod_service.dod_token_canister.clone())
            .ok_or_else(|| "No service found".to_string())
    })
}

pub fn get_dod_block_account() -> Result<[u8; 32], String> {
    CONFIG.with(|config| {
        let config = config.borrow();
        config
            .dod_service
            .as_ref()
            .map(|dod_service| {
                vec_to_u832(dod_service.dod_block_sub_account.clone()).map(|s| s.clone())
            })
            .unwrap_or_else(|| Err("No service found".to_string()))
    })
}
pub fn get_block_time_interval() -> Result<u64, String> {
    CONFIG.with(|config| {
        config
            .borrow()
            .dod_service
            .as_ref()
            .map(|dod_service| dod_service.block_time_interval)
            .ok_or_else(|| "No service found".to_string())
    })
}

pub fn get_difficulty_adjust_epoch() -> Result<u64, String> {
    CONFIG.with(|config| {
        config
            .borrow()
            .dod_service
            .as_ref()
            .map(|dod_service| dod_service.difficulty_adjust_epoch)
            .ok_or_else(|| "No service found".to_string())
    })
}

pub fn get_default_rewards() -> Result<u64, String> {
    CONFIG.with(|config| {
        config
            .borrow()
            .dod_service
            .as_ref()
            .map(|dod_service| dod_service.default_rewards)
            .ok_or_else(|| "No service found".to_string())
    })
}

pub fn get_start_difficulty() -> Result<Bitwork, String> {
    CONFIG.with(|config| {
        config
            .borrow()
            .dod_service
            .as_ref()
            .map(|dod_service| dod_service.start_difficulty.clone())
            .ok_or_else(|| "No service found".to_string())
    })
}

pub fn get_halving_settings() -> Option<HalvingSettings> {
    CONFIG.with(|config| {
        config
            .borrow()
            .dod_service
            .as_ref()
            .and_then(|dod_service| dod_service.halving_settings.clone())
    })
}

pub fn set_halving_settings(setting: HalvingSettings) -> Result<(), String> {
    CONFIG.with(|config| {
        config
            .borrow_mut()
            .dod_service
            .as_mut()
            .map(|dod_service| {
                dod_service.halving_settings = setting.into();
                Ok(())
            })
            .unwrap_or_else(|| Err("No service found".to_string()))
    })
}

pub fn get_consider_decrease() -> Result<Option<u64>, String> {
    CONFIG.with(|config| {
        config
            .borrow()
            .dod_service
            .as_ref()
            .map(|dod_service| dod_service.consider_decrease)
            .ok_or_else(|| "No service found".to_string())
    })
}

pub fn get_consider_increase() -> Result<Option<u64>, String> {
    CONFIG.with(|config| {
        config
            .borrow()
            .dod_service
            .as_ref()
            .map(|dod_service| dod_service.consider_increase)
            .ok_or_else(|| "No service found".to_string())
    })
}

pub fn set_consider_decrease(consider_decrease: Option<u64>) -> Result<(), String> {
    CONFIG.with(|config| {
        config
            .borrow_mut()
            .dod_service
            .as_mut()
            .map(|dod_service| {
                dod_service.consider_decrease = consider_decrease;
                Ok(())
            })
            .unwrap_or_else(|| Err("No service found".to_string()))
    })
}

pub fn set_consider_increase(consider_increase: Option<u64>) -> Result<(), String> {
    CONFIG.with(|config| {
        let mut config = config.borrow_mut();
        config
            .dod_service
            .as_mut()
            .map(|dod_service| {
                dod_service.consider_increase = consider_increase;
                Ok(())
            })
            .unwrap_or_else(|| Err("No service found".to_string()))
    })
}

pub fn set_difficulty_adjust_epoch(difficulty_adjust_epoch: u64) -> Result<(), String> {
    CONFIG.with(|config| {
        let mut config = config.borrow_mut();
        config
            .dod_service
            .as_mut()
            .map(|dod_service| {
                dod_service.difficulty_adjust_epoch = difficulty_adjust_epoch;
                Ok(())
            })
            .unwrap_or_else(|| Err("No service found".to_string()))
    })
}

pub fn get_current_halving_ratio(block: Height, halving_settings: HalvingSettings) -> f64 {
    let cycle = block / halving_settings.interval; // halving cycle;
    halving_settings.ratio.powi(cycle as i32)
}

#[cfg(test)]
mod test {
    use crate::service::config::{get_current_halving_ratio, get_halving_settings};
    use dod_utils::types::HalvingSettings;

    #[test]
    pub fn test_get_current_halving_ratio() {
        let last_block = 21000;
        let s = HalvingSettings {
            interval: 21000,
            ratio: 0.5,
        };
        let r = get_current_halving_ratio(last_block, s);
        println!("r: {:?}", r);
        assert_eq!(r, 0.5);

        let f = (10000 as f64 * r).floor() as u64;
        println!("f: {:?}", f);
    }
}
