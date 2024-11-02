use std::fmt;

#[derive(Debug)]
pub enum DodError {
    AddressTypeNotSupported,
    AddressFormatError(String),
    DecodingError(hex::FromHexError),
    SignatureFormatError(String),
    InvalidSignature,
    InvalidRecoveryId,
    PublicKeyRecoveryFailure,
}

impl From<hex::FromHexError> for DodError {
    fn from(err: hex::FromHexError) -> Self {
        DodError::DecodingError(err)
    }
}

impl fmt::Display for DodError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DodError::AddressFormatError(e) => write!(f, "Address format error: {}", e),
            DodError::DecodingError(e) => write!(f, "Decoding error: {}", e),
            DodError::SignatureFormatError(e) => write!(f, "Signature format error: {}", e),
            DodError::InvalidSignature => write!(f, "Invalid signature"),
            DodError::InvalidRecoveryId => write!(f, "Invalid recovery ID"),
            DodError::PublicKeyRecoveryFailure => {
                write!(f, "Public key recovery failure")
            }
            DodError::AddressTypeNotSupported => {
                write!(f, "Address type not supported")
            }
        }
    }
}

impl From<DodError> for String {
    fn from(error: DodError) -> Self {
        error.to_string()
    }
}
