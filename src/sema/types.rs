#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Type {
    Number,
    String,
    Bool,
    None,
    Procedure,
    List,
    Map,
    NumberWithDimension(String),
    Unknown,
}
