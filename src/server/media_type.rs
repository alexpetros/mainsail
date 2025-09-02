#[derive(Debug)]
pub struct MediaType<'a> {
    pub media_type: &'a str,
    pub media_subtype: &'a str,
}

#[derive(Debug, PartialEq)]
pub struct ParseMediaTypeError;

impl<'a> MediaType<'a> {
    pub fn get_list(string: &'a str) -> Vec<MediaType<'a>> {
        let split = string.split(',');
        split.filter_map(|t| { MediaType::try_from(t.trim()).ok() }).collect()
    }
}

impl<'a> TryFrom<&'a str> for MediaType<'a> {
    type Error = ParseMediaTypeError;

    fn try_from(string: &'a str) -> Result<Self, Self::Error> {
        let split = string.split_once('/').ok_or(ParseMediaTypeError)?;
        let media_type = split.0;
        // There's probably a less egregious way to cut off everything after the ';'
        // Eventually it might be necessary to parse the options, you can do that here instead
        let media_subtype = split.1.split_once(';').map(|s| s.0).unwrap_or(split.1);
        Ok(MediaType { media_type, media_subtype })
    }
}

impl PartialEq for MediaType<'_> {
    fn eq(&self, other: &Self) -> bool {
        // Media types are equal if their main type is the same, and their subtype is the same
        // (or one is a wild card)
        self.media_type == other.media_type &&
            (
                self.media_subtype == "*" ||
                    other.media_subtype == "*" ||
                    self.media_subtype == other.media_subtype
            )
    }
}

#[cfg(test)]
mod tests {
    use crate::server::media_type::MediaType;

    #[test]
    fn media_type_equal() {
        let first = MediaType::try_from("application/json");
        let second = MediaType::try_from("application/json");
        assert_eq!(first, second);
    }

    #[test]
    fn media_type_equal_with_wildcard() {
        let first = MediaType::try_from("text/*");
        let second = MediaType::try_from("text/html");
        assert_eq!(first, second);
    }

    #[test]
    fn media_type_equal_with_wildcard_other() {
        let first = MediaType::try_from("text/html");
        let second = MediaType::try_from("text/*");
        assert_eq!(first, second);
    }

    #[test]
    fn media_type_not_equal() {
        let first = MediaType::try_from("application/json");
        let second = MediaType::try_from("application/ld+json");
        assert_ne!(first, second);
    }

    #[test]
    fn media_type_not_equal_with_wildcard() {
        let first = MediaType::try_from("application/json");
        let second = MediaType::try_from("text/*");
        assert_ne!(first, second);
    }

    #[test]
    fn media_type_cuts_off_options() {
        let first = MediaType::try_from("application/json;q=0.8");
        let second = MediaType::try_from("application/json");
        assert_eq!(first, second);
    }

    #[test]
    fn media_type_get_list() {
        let list = MediaType::get_list("application/json, text/html");
        let first = MediaType::try_from("application/json").unwrap();
        let second = MediaType::try_from("text/html").unwrap();

        assert_eq!(list[0], first);
        assert_eq!(list[1], second);
    }

    #[test]
    fn media_type_get_list_no_space() {
        let list = MediaType::get_list("application/json,text/html");
        let first = MediaType::try_from("application/json").unwrap();
        let second = MediaType::try_from("text/html").unwrap();

        assert_eq!(list[0], first);
        assert_eq!(list[1], second);
    }

    #[test]
    fn media_type_get_list_malformatted() {
        let list = MediaType::get_list("applicationjson");
        assert_eq!(list.len(), 0);
    }
}
