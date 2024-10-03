use crate::protocol::{vec_to_u832, DodAssets, DodOps, ParsedEnvelope, MAGIC_VALUE};
use bitcoin::key::Secp256k1;
use bitcoin::key::XOnlyPublicKey;
use bitcoin::psbt::{Prevouts, Psbt};
use bitcoin::sighash::SighashCache;
use bitcoin::taproot::TapTweakHash;
use bitcoin::Network::{Bitcoin, Testnet};
use bitcoin::{secp256k1, Address, AddressType, Network, ScriptBuf};
use std::str::FromStr;

pub struct AddressInfo {
    pub address: String,
    pub script_buf: ScriptBuf,
    pub network: Network,
    pub address_type: AddressType,
}

pub fn get_script_from_address(address: String) -> Result<AddressInfo, String> {
    let mut network = Bitcoin;
    let mut address_type = AddressType::P2tr;

    if address.starts_with("bc1q") {
        address_type = AddressType::P2wpkh;
        network = Bitcoin;
    } else if address.starts_with("bc1p") {
        address_type = AddressType::P2tr;
        network = Bitcoin;
    } else if address.starts_with('1') {
        address_type = AddressType::P2pkh;
        network = Bitcoin;
    } else if address.starts_with('3') {
        address_type = AddressType::P2sh;
        network = Bitcoin;
    } else if address.starts_with("tb1q") {
        address_type = AddressType::P2wpkh;
        network = Testnet;
    } else if address.starts_with('m') || address.starts_with('n') {
        address_type = AddressType::P2pkh;
        network = Testnet;
    } else if address.starts_with('2') {
        address_type = AddressType::P2sh;
        network = Testnet;
    } else if address.starts_with("tb1p") {
        address_type = AddressType::P2tr;
        network = Testnet;
    }
    let addr = Address::from_str(address.as_str())
        .map_err(|e| format!("Cannot gen address {:?}", e).to_string())?;

    let addr_checked = addr
        .clone()
        .require_network(network)
        .map_err(|e| format!("Cannot require network {:?}", e).to_string())?;

    Ok(AddressInfo {
        address: addr_checked.to_string(),
        script_buf: addr_checked.script_pubkey(),
        network,
        address_type,
    })
}

pub fn checked_signed_commit_psbt_b64(
    psbt_b64: &str,
    pubkey: Vec<u8>,
    input_hash: Vec<u8>,
) -> Result<(String, ScriptBuf), String> {
    let Ok(mut psbt) = Psbt::from_str(psbt_b64) else {
        return Err("Cannot decode psbt".to_string());
    };
    let err = None;
    let Ok(xonly) = XOnlyPublicKey::from_slice(&pubkey[1..]) else {
        return Err("Cannot decode xonly".to_string());
    };
    psbt.inputs[0].tap_internal_key = Some(xonly);
    psbt_verifier(psbt.clone(), err.clone());
    if err.clone().is_none() {
        let tx = psbt.clone().extract_tx();
        let id = tx.txid();

        if psbt.inputs[0].witness_utxo.is_some()
            && psbt.inputs[0].clone().witness_utxo.unwrap().value == MAGIC_VALUE
            && tx.input[0].previous_output.txid.to_string() == hex::encode(input_hash)
            && tx.input[0].previous_output.vout == 0
            && tx.output[0].script_pubkey.is_v1_p2tr()
        {
            Ok((id.to_string(), tx.output[0].clone().script_pubkey))
        } else {
            Err("Tap internal key is not match".to_string())
        }
    } else {
        Err(err.clone().unwrap())
    }
}

pub fn check_signed_reveal_psbt(
    psbt_b64: &str,
    prev_script: ScriptBuf,
    pubkey: Vec<u8>,
    commit_id: String,
    miner_address: String,
) -> Result<(), String> {
    let Ok(psbt) = Psbt::from_str(psbt_b64) else {
        return Err("Cannot decode psbt".to_string());
    };
    let err = None;
    psbt_verifier(psbt.clone(), err.clone());
    if err.clone().is_none() {
        let tx = psbt.clone().extract_tx();
        let staker = &pubkey[1..];

        let AddressInfo { script_buf, .. } = get_script_from_address(miner_address)?;

        if psbt.inputs[0].witness_utxo.is_some()
            && psbt.inputs[0].clone().witness_utxo.unwrap().script_pubkey == prev_script
            && tx.input[0].previous_output.txid.to_string() == commit_id
            && tx.input[0].previous_output.vout == 0
            && tx.output[0].script_pubkey.is_v1_p2tr()
            && tx.output[0].script_pubkey == script_buf
        {
            let parsed = ParsedEnvelope::from_transaction(&tx);

            if parsed.len() != 1 {
                Err("ParsedEnvelope length is not 1".to_string())
            } else {
                let p = parsed[0].clone();
                if p.op_type != Some(DodOps::Mine) {
                    return Err("Op type is not mine".to_string());
                }
                if let Some(payload) = p.payload {
                    if payload.t != DodAssets::DMT {
                        return Err("Asset type is not DMT".to_string());
                    }
                    if payload.dmt.is_none() {
                        return Err("DMT is none".to_string());
                    }
                }

                if p.stakers.len() != 1
                    && hex::encode(p.stakers[0].clone())
                        != hex::encode(vec_to_u832(staker.to_vec())?)
                {
                    return Err("Staker is not match".to_string());
                }
                Ok(())
            }
        } else {
            Err("Validation failed, block hash might be changed".to_string())
        }
    } else {
        Err(err.clone().unwrap())
    }
}

pub fn psbt_verifier(decoded_psbt: Psbt, mut err: Option<String>) -> Option<String> {
    let secp = Secp256k1::new();
    let prevouts: Vec<_> = decoded_psbt
        .inputs
        .iter()
        .map(|input| input.witness_utxo.as_ref().unwrap())
        .collect();
    let prevouts = Prevouts::All(&prevouts);
    for (i, input) in decoded_psbt.inputs.iter().enumerate() {
        if let Some(_) = &input.witness_utxo {
            // let amount = witness_utxo.value;
            // let script_pubkey = &witness_utxo.script_pubkey;

            // If the input is Taproot
            if !input.tap_script_sigs.is_empty() {
                err = Some("We only support tap key sig".to_string());
                break;
            } else if input.tap_key_sig.is_some() && input.tap_internal_key.is_some() {
                let mut cache = SighashCache::new(&decoded_psbt.unsigned_tx);
                let hash_type = input.tap_key_sig.unwrap().hash_ty;
                let sig = input.tap_key_sig.unwrap().sig;

                let sighash = cache.taproot_key_spend_signature_hash(i, &prevouts, hash_type);
                match sighash {
                    Ok(sighash) => {
                        let message = secp256k1::Message::from(sighash);
                        let (tweaked_key, _) = input
                            .tap_internal_key
                            .unwrap()
                            .add_tweak(
                                &secp,
                                &TapTweakHash::from_key_and_tweak(
                                    input.tap_internal_key.unwrap(),
                                    None,
                                )
                                .to_scalar(),
                            )
                            .unwrap();
                        match secp.verify_schnorr(&sig, &message, &tweaked_key) {
                            Ok(_) => {
                                println!("Signature verified");
                                continue;
                            }
                            Err(e) => {
                                err = Some(format!(
                                    "tap_key_sig Input {}: Taproot signature is invalid: {:?}",
                                    i,
                                    e.to_string()
                                ));
                                println!("err {:?}", err);
                                break;
                            }
                        }
                    }
                    Err(e) => {
                        err = Some(format!("Input {}: {}", i, e));
                        println!("err {:?}", err);
                        break;
                    }
                }
            } else {
                err = Some("We only support tap key sig".to_string());
                break;
            }
        }
    }
    err
}

#[cfg(test)]
mod test {
    use crate::verifier::{check_signed_reveal_psbt, checked_signed_commit_psbt_b64};

    #[test]
    pub fn test_commit() {
        let commit_psbt ="cHNidP8BAKQBAAAAAY+eca9rbNhkzTyob8O0i55rDyVgToBUzetfGuLDuqSVAAAAAAD9////A0wFAAAAAAAAIlEgdHgSymyd9yRSOxAvVACefwEo5N7+RC772lRiykp4G+YAAAAAAAAAABJqEI+qKr3wRCQD7Dbs+6FJFegYUQEAAAAAACJRIGHwI7GSVAtAtFnpqmKu3OuHTm6lmXI9IapydOXdw76JAAAAAAABASuYVwEAAAAAACJRIGHwI7GSVAtAtFnpqmKu3OuHTm6lmXI9IapydOXdw76JAQhCAUDClOeS/Wtorlx9j3HUwM7ffXK0DPWoQx9huP5iePsOmMgf3BK1KSJ3EmGL7GWTP4OaI5ulcqDyVyZqNBIt/cXoAAAAAA==";
        let res = checked_signed_commit_psbt_b64(
            commit_psbt,
            hex::decode("02afee55a2cdcb6c47a593d629b04e13399354d348a3d84ad19310e2b6396e7237")
                .unwrap(),
            hex::decode("95a4bac3e21a5febcd54804e60250f6b9e8bb4c36fa83ccd64d86c6baf719e8f")
                .unwrap(),
        )
        .unwrap();

        let reveal_psbt = "cHNidP8BAF4BAAAAAQGvInD6DU8qnfn7O4oMVah3ofKqe2IjsBUqb0EXU5yPAAAAAAD9////ASICAAAAAAAAIlEgYfAjsZJUC0C0WemqYq7c64dObqWZcj0hqnJ05d3DvokAAAAAAAEBK0wFAAAAAAAAIlEgdHgSymyd9yRSOxAvVACefwEo5N7+RC772lRiykp4G+YBCLcDQO6qytI7SOuVrLV0Qr1is1fMCgN3E84TytiUqYu7xw0aHFfPHZv5I3PHRrhzwcRUtWRbmCsNvHxqPpEz64vJeNNSIK/uVaLNy2xHpZPWKbBOEzmTVNNIo9hK0ZMQ4rY5bnI3rABjA2RvZAFZJqJhdGNETVRjZG10o2NibGsAZHRpbWUaZVPxAGVub25jZRoAmJZ/aCHBr+5Vos3LbEelk9YpsE4TOZNU00ij2ErRkxDitjlucjcAAA==";
        let res_reveal = check_signed_reveal_psbt(
            reveal_psbt,
            res.1.clone(),
            hex::decode("02afee55a2cdcb6c47a593d629b04e13399354d348a3d84ad19310e2b6396e7237")
                .unwrap(),
            res.0.clone(),
            "tb1pv8cz8vvj2s95pdzeax4x9tkuawr5um49n9er6gd2wf6wthwrh6ysqnkcq9".to_string(),
        )
        .unwrap();
        println!("commit check {:?}, reveal check {:?}", res, res_reveal);
    }

    #[test]
    pub fn test_reveal() {}
}
