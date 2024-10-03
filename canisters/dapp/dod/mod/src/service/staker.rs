use crate::common::CYCLES_BURNER_FEE;
use crate::memory::STAKERS;
use crate::types::UserDetail;
use candid::{Nat, Principal};
use ic_ledger_types::Subaccount;
use ic_stable_structures::storable::Blob;

pub fn user_set_burnrate(user: Principal, burn_rate: u128) -> Result<(), String> {
    if burn_rate < CYCLES_BURNER_FEE {
        return Err("Burn rate too low".to_string());
    }
    let blob29 = Blob::<29>::try_from(user.as_slice()).expect("error transformation");
    STAKERS.with(|v| {
        let mut _v = v.borrow_mut();
        let user = _v.get(&blob29);
        match user {
            None => Err("User not found".to_string()),
            Some(r) => {
                _v.insert(
                    blob29,
                    UserDetail {
                        cycle_burning_rate: burn_rate,
                        ..r.clone()
                    },
                );
                Ok(())
            }
        }
    })
}

pub fn get_user_burnrate(user: Principal) -> Result<(u128, Nat), String> {
    let blob29 = Blob::<29>::try_from(user.as_slice()).expect("error transformation");
    STAKERS.with(|v| {
        let _v = v.borrow();
        let user = _v.get(&blob29);
        match user {
            None => Err("User not found".to_string()),
            Some(r) => Ok((r.cycle_burning_rate, r.balance)),
        }
    })
}

pub fn register_user(user: Principal) -> Result<(), String> {
    let blob29 = Blob::<29>::try_from(user.as_slice()).expect("error transformation");
    let user_exist = STAKERS.with_borrow(|v| v.get(&blob29));

    if user_exist.is_none() {
        STAKERS.with(|v| {
            v.borrow_mut().insert(
                blob29,
                UserDetail {
                    principal: user.clone(),
                    subaccount: Subaccount::from(user.clone()),
                    balance: Nat::from(0u128),
                    claimed_dod: 0,
                    total_dod: 0,
                    cycle_burning_rate: 0,
                },
            );
        });
    }
    Ok(())
}
