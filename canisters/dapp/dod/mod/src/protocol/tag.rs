use std::collections::{BTreeMap, HashMap, VecDeque};
use std::mem;

use crate::protocol::varint;
use bitcoin::constants::MAX_SCRIPT_ELEMENT_SIZE;
use bitcoin::script;

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Tag {
    #[allow(unused)]
    Mine = 89,
    #[allow(unused)]
    Version = 99,
    #[allow(unused)]
    Metadata = 21,
    #[allow(unused)]
    Note = 15,
    #[allow(unused)]
    Nop = 255,
}

impl Tag {
    fn chunked(self) -> bool {
        matches!(self, Self::Metadata)
    }

    pub(crate) fn bytes(self) -> [u8; 1] {
        [self as u8]
    }

    #[allow(unused)]
    pub(crate) fn append(self, builder: &mut script::Builder, value: &Option<Vec<u8>>) {
        if let Some(value) = value {
            let mut tmp = script::Builder::new();
            mem::swap(&mut tmp, builder);

            if self.chunked() {
                for chunk in value.chunks(MAX_SCRIPT_ELEMENT_SIZE) {
                    tmp = tmp
                        .push_slice::<&script::PushBytes>(
                            self.bytes().as_slice().try_into().unwrap(),
                        )
                        .push_slice::<&script::PushBytes>(chunk.try_into().unwrap());
                }
            } else {
                tmp = tmp
                    .push_slice::<&script::PushBytes>(self.bytes().as_slice().try_into().unwrap())
                    .push_slice::<&script::PushBytes>(value.as_slice().try_into().unwrap());
            }

            mem::swap(&mut tmp, builder);
        }
    }

    #[allow(unused)]
    pub(crate) fn append_array(self, builder: &mut script::Builder, values: &Vec<Vec<u8>>) {
        let mut tmp = script::Builder::new();
        mem::swap(&mut tmp, builder);

        for value in values {
            tmp = tmp
                .push_slice::<&script::PushBytes>(self.bytes().as_slice().try_into().unwrap())
                .push_slice::<&script::PushBytes>(value.as_slice().try_into().unwrap());
        }

        mem::swap(&mut tmp, builder);
    }

    pub(crate) fn take(self, fields: &mut BTreeMap<&[u8], Vec<&[u8]>>) -> Option<Vec<u8>> {
        if self.chunked() {
            let value = fields.remove(self.bytes().as_slice())?;

            if value.is_empty() {
                None
            } else {
                Some(value.into_iter().flatten().cloned().collect())
            }
        } else {
            let values = fields.get_mut(self.bytes().as_slice())?;

            if values.is_empty() {
                None
            } else {
                let value = values.remove(0).to_vec();

                if values.is_empty() {
                    fields.remove(self.bytes().as_slice());
                }

                Some(value)
            }
        }
    }

    #[allow(unused)]
    pub(crate) fn take_with<const N: usize, T>(
        self,
        fields: &mut HashMap<u128, VecDeque<u128>>,
        with: impl Fn([u128; N]) -> Option<T>,
    ) -> Option<T> {
        let field = fields.get_mut(&self.into())?;

        let mut values: [u128; N] = [0; N];

        for (i, v) in values.iter_mut().enumerate() {
            *v = *field.get(i)?;
        }

        let value = with(values)?;

        field.drain(0..N);

        if field.is_empty() {
            fields.remove(&self.into()).unwrap();
        }

        Some(value)
    }

    #[allow(unused)]
    pub(crate) fn encode<const N: usize>(self, values: [u128; N], payload: &mut Vec<u8>) {
        for value in values {
            varint::encode_to_vec(self.into(), payload);
            varint::encode_to_vec(value, payload);
        }
    }

    #[allow(unused)]
    pub(crate) fn encode_option<T: Into<u128>>(self, value: Option<T>, payload: &mut Vec<u8>) {
        if let Some(value) = value {
            self.encode([value.into()], payload)
        }
    }

    #[allow(unused)]
    pub(crate) fn take_array(self, fields: &mut BTreeMap<&[u8], Vec<&[u8]>>) -> Vec<Vec<u8>> {
        fields
            .remove(self.bytes().as_slice())
            .unwrap_or_default()
            .into_iter()
            .map(|v| v.to_vec())
            .collect()
    }
}

impl From<Tag> for u128 {
    fn from(tag: Tag) -> Self {
        tag as u128
    }
}

impl PartialEq<u128> for Tag {
    fn eq(&self, other: &u128) -> bool {
        u128::from(*self) == *other
    }
}
