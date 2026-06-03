use thiserror::Error;

pub type AdcResult<T> = Result<T, AdcError>;

#[derive(Debug, Error)]
pub enum AdcError {
    #[error("profile parse failed: {0}")]
    ProfileParse(String),
    #[error("profile validation failed: {0}")]
    ProfileValidation(String),
    #[error("artifact operation failed: {0}")]
    Artifact(String),
}
