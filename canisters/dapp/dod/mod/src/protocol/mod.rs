pub mod tag;
pub mod varint;

use std::collections::BTreeMap;
use std::iter::Peekable;

use bitcoin::opcodes::all::{OP_CHECKSIG, OP_CLTV};
use bitcoin::script::Instruction::{Op, PushBytes};
use bitcoin::script::{Instruction, Instructions};
use bitcoin::{opcodes, script, Script, Transaction};
use candid::CandidType;
use serde::{Deserialize, Serialize};
use tag::Tag;

pub(crate) const PROTOCOL_ID: [u8; 3] = *b"dod";
pub const MAGIC_VALUE: u64 = 87960;

#[derive(PartialEq, Clone, Serialize, Deserialize, Debug, Eq, Default)]
pub enum DodAssets {
    #[default]
    DMT,
}

#[derive(Default, PartialEq, Clone, Serialize, Deserialize, Debug, Eq)]
pub struct DodStruct {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub n: Option<String>,
    pub t: DodAssets,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dmt: Option<DodMining>,
}

#[derive(Debug, Clone, Serialize, Deserialize, CandidType, Eq, PartialEq)]
pub struct DodMining {
    pub time: u32,
    pub nonce: u32,
}

#[derive(PartialEq, Clone, Serialize, Deserialize, Debug, Eq)]
pub enum DodOps {
    Mine,
}

type Result2<T> = Result<T, script::Error>;
type RawEnvelope = Envelope<Vec<Vec<u8>>>;
pub type ParsedEnvelope = Envelope<DodStruct>;

#[derive(Default, PartialEq, Clone, Serialize, Deserialize, Debug, Eq)]
pub struct Envelope<T> {
    pub payload: Option<T>,
    pub op_type: Option<DodOps>,
    pub stakers: Vec<[u8; 32]>,
}

pub fn decode_cbor_payload(slice: &[u8]) -> Option<DodStruct> {
    let res = serde_cbor::from_slice::<DodStruct>(slice);
    match res {
        Ok(r) => {
            if r.t != DodAssets::DMT {
                return None;
            }
            return Some(r.clone());
        }
        Err(_) => None,
    }
}

impl From<RawEnvelope> for ParsedEnvelope {
    fn from(envelope: RawEnvelope) -> Self {
        let mut fields: BTreeMap<&[u8], Vec<&[u8]>> = BTreeMap::new();

        if envelope.payload.is_none() {
            Self {
                op_type: None,
                payload: None,
                stakers: vec![],
            }
        } else {
            let payloads = envelope.payload.unwrap();
            if payloads.len() == 1 || payloads.is_empty() {
                return Self {
                    op_type: None,
                    stakers: vec![],
                    payload: None,
                };
            }
            let key = payloads[0].clone();
            let mut values: Vec<u8> = vec![];

            for value in payloads[1..].to_vec() {
                values.extend_from_slice(value.as_slice());
            }
            fields
                .entry(key.as_slice())
                .or_default()
                .push(values.as_slice());

            let duplicate_field = fields.iter().any(|(_key, values)| values.len() > 1);

            if duplicate_field {
                return Self {
                    op_type: None,
                    stakers: vec![],
                    payload: None,
                };
            }

            let op_mine = Tag::Mine.take(&mut fields);

            let op_type = match (op_mine.is_some(),) {
                (true,) => Some(DodOps::Mine),
                _ => None,
            };

            let payload = match op_type {
                Some(DodOps::Mine) => decode_cbor_payload(op_mine.unwrap().as_slice()),
                _ => None,
            };

            Self {
                op_type,
                payload,
                stakers: envelope.stakers,
            }
        }
    }
}

impl ParsedEnvelope {
    pub fn from_transaction(transaction: &Transaction) -> Vec<Self> {
        RawEnvelope::from_transaction(transaction)
            .into_iter()
            .map(|envelope| envelope.into())
            .collect()
    }
}

impl RawEnvelope {
    pub(crate) fn from_transaction(transaction: &Transaction) -> Vec<Self> {
        let mut envelopes = Vec::new();

        for (i, input) in transaction.input.iter().enumerate() {
            if let Some(tapscript) = input.witness.tapscript() {
                if let Ok(input_envelopes) = Self::from_tapscript(tapscript, i) {
                    envelopes.extend(input_envelopes);
                }
            }
        }

        envelopes
    }

    fn from_tapscript(tapscript: &Script, input: usize) -> Result2<Vec<Self>> {
        let mut envelopes = Vec::new();

        let mut instructions = tapscript.instructions().peekable();

        let mut stuttered = false;

        let mut stakers = Vec::new();
        let mut lock_time = None;

        while let Some(instruction) = instructions.next().transpose()? {
            match instruction {
                PushBytes(r) => {
                    if r.len() == 32 && instructions.peek() == Some(&Ok(Op(OP_CHECKSIG))) {
                        let staker = vec_to_u832(r.as_bytes().to_vec()).unwrap();
                        stakers.push(staker);
                    }
                    if r.len() == 2 && instructions.peek() == Some(&Ok(Op(OP_CLTV))) {
                        let mut b = r.as_bytes().to_vec();
                        b.reverse();
                        lock_time = u128::from_str_radix(hex::encode(b).as_str(), 16)
                            .map_or_else(|_| None, |v| Some(v));
                    }
                }
                _ => {}
            }

            if instruction == PushBytes((&[]).into()) {
                let (stutter, envelope) = Self::from_instructions(
                    &mut instructions,
                    input,
                    envelopes.len(),
                    stuttered,
                    stakers.clone(),
                    lock_time,
                )?;
                if let Some(envelope) = envelope {
                    envelopes.push(envelope);
                } else {
                    stuttered = stutter;
                }
            }
        }
        Ok(envelopes)
    }

    fn accept(
        instructions: &mut Peekable<Instructions>,
        instruction: Instruction,
    ) -> Result2<bool> {
        if instructions.peek() == Some(&Ok(instruction)) {
            instructions.next().transpose()?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn from_instructions(
        instructions: &mut Peekable<Instructions>,
        _input: usize,
        _offset: usize,
        _stutter: bool,
        _stakers: Vec<[u8; 32]>,
        _lock_time: Option<u128>,
    ) -> Result2<(bool, Option<Self>)> {
        if !Self::accept(instructions, Op(opcodes::all::OP_IF))? {
            let stutter = instructions.peek() == Some(&Ok(PushBytes((&[]).into())));
            return Ok((stutter, None));
        }

        if !Self::accept(instructions, PushBytes((&PROTOCOL_ID).into()))? {
            let stutter = instructions.peek() == Some(&Ok(PushBytes((&[]).into())));
            return Ok((stutter, None));
        }

        let mut _push_num = false;

        let mut payload = Vec::new();

        loop {
            match instructions.next().transpose()? {
                None => return Ok((false, None)),
                Some(Op(opcodes::all::OP_ENDIF)) => {
                    return Ok((
                        false,
                        Some(Envelope {
                            payload: Some(payload),
                            stakers: _stakers,
                            op_type: None,
                        }),
                    ));
                }
                Some(Op(opcodes::all::OP_PUSHNUM_NEG1)) => {
                    _push_num = true;
                    payload.push(vec![0x81]);
                }
                Some(Op(opcodes::all::OP_PUSHNUM_1)) => {
                    _push_num = true;
                    payload.push(vec![1]);
                }
                Some(Op(opcodes::all::OP_PUSHNUM_2)) => {
                    _push_num = true;
                    payload.push(vec![2]);
                }
                Some(Op(opcodes::all::OP_PUSHNUM_3)) => {
                    _push_num = true;
                    payload.push(vec![3]);
                }
                Some(Op(opcodes::all::OP_PUSHNUM_4)) => {
                    _push_num = true;
                    payload.push(vec![4]);
                }
                Some(Op(opcodes::all::OP_PUSHNUM_5)) => {
                    _push_num = true;
                    payload.push(vec![5]);
                }
                Some(Op(opcodes::all::OP_PUSHNUM_6)) => {
                    _push_num = true;
                    payload.push(vec![6]);
                }
                Some(Op(opcodes::all::OP_PUSHNUM_7)) => {
                    _push_num = true;
                    payload.push(vec![7]);
                }
                Some(Op(opcodes::all::OP_PUSHNUM_8)) => {
                    _push_num = true;
                    payload.push(vec![8]);
                }
                Some(Op(opcodes::all::OP_PUSHNUM_9)) => {
                    _push_num = true;
                    payload.push(vec![9]);
                }
                Some(Op(opcodes::all::OP_PUSHNUM_10)) => {
                    _push_num = true;
                    payload.push(vec![10]);
                }
                Some(Op(opcodes::all::OP_PUSHNUM_11)) => {
                    _push_num = true;
                    payload.push(vec![11]);
                }
                Some(Op(opcodes::all::OP_PUSHNUM_12)) => {
                    _push_num = true;
                    payload.push(vec![12]);
                }
                Some(Op(opcodes::all::OP_PUSHNUM_13)) => {
                    _push_num = true;
                    payload.push(vec![13]);
                }
                Some(Op(opcodes::all::OP_PUSHNUM_14)) => {
                    _push_num = true;
                    payload.push(vec![14]);
                }
                Some(Op(opcodes::all::OP_PUSHNUM_15)) => {
                    _push_num = true;
                    payload.push(vec![15]);
                }
                Some(Op(opcodes::all::OP_PUSHNUM_16)) => {
                    _push_num = true;
                    payload.push(vec![16]);
                }
                Some(PushBytes(push)) => {
                    payload.push(push.as_bytes().to_vec());
                }
                Some(_) => return Ok((false, None)),
            }
        }
    }
}

pub fn vec_to_u832(req: Vec<u8>) -> Result<[u8; 32], String> {
    if req.len() != 32 {
        return Err("Salt length should be 32".to_string());
    }
    let mut salt_bytes = [0u8; 32];

    for i in 0..32 {
        salt_bytes[i] = req[i.clone()]
    }
    Ok(salt_bytes.clone())
}

pub fn vec_to_u84(req: Vec<u8>) -> Result<[u8; 4], String> {
    if req.len() != 4 {
        return Err("Salt length should be 4".to_string());
    }
    let mut salt_bytes = [0u8; 4];

    for i in 0..4 {
        salt_bytes[i] = req[i.clone()]
    }
    Ok(salt_bytes.clone())
}
