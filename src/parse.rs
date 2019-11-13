pub enum Event {
    Message {
        ty: String,
        text: String,
    },
    Comment(String),
}

impl std::str::FromStr for Event {
    type Err = Box<dyn std::error::Error>;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self::Comment(s.into()))
    }
}

enum Field {
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
