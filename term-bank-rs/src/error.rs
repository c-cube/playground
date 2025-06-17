#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("bank is full")]
    Full,
    #[error("invalid index {0}")]
    InvalidIndex(u32),

    #[error("wrong generation for index {0}")]
    WrongGeneration(u32),
    #[error("slice is too big")]
    SliceTooBig,
}

pub type Result<T> = std::result::Result<T, Error>;
