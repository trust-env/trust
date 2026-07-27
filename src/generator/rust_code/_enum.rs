pub struct EnumVariant {
    pub name: String,
    pub value: Option<String>,
}

pub struct Enum {
    pub name: String,
    pub variants: Vec<EnumVariant>,
}
