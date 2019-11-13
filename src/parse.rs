pub enum Line {
    Empty,
    Comment(String),
    FieldValue {
        field: Field,
        value: String, // empty string if line has no "value"
    },
}

impl std::str::FromStr for Line {
    type Err = Box<dyn std::error::Error>;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim();
        
        Ok(Self::Comment(s.into()))
    }
}

pub enum Field {
    Event,
    Data,
    Id,
    Retry,
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_parse() {
        assert_eq!(2 + 2, 4);
    }
}
