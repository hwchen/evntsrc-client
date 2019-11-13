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

        if s.is_empty() {
            return Ok(Self::Empty);
        }

        let split_idx = s.chars().position(|c| c == ':');

        let (field, value) = if let Some(idx) = split_idx {
            // leading colon, return comment
            if idx == 0 {
                return Ok(Self::Comment(s.into()));
            }

            let (field, value) = s.split_at(idx);

            let value = value.trim_start_matches(':').trim();

            (field, value)
        } else {
            (s, "")
        };

        let field = field.parse()?;
        let value = value.to_owned();

        Ok(Self::FieldValue {
            field,
            value,
        })
    }
}

pub enum Field {
    Event,
    Data,
    Id,
    Retry,
}

impl std::str::FromStr for Field {
    type Err = Box<dyn std::error::Error>;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "event" => Ok(Self::Event),
            "data" => Ok(Self::Data),
            "id" => Ok(Self::Id),
            "retry" => Ok(Self::Retry),
            _ => Err("no field matched".into()),
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_parse() {
        assert_eq!(2 + 2, 4);
    }
}
