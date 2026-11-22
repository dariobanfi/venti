//! A small reader for the XML the province's service writes.
//!
//! The workspace has no XML library and gets none for one adapter. The
//! service's answers use little of XML: a declaration, elements with
//! attributes, text, empty elements written `<a />`, and namespaces declared
//! on the root and never used as prefixes of elements. This reader reads
//! that, and the few things a well-formed document may hold besides
//! (comments, processing instructions, CDATA sections, character and the
//! five predefined entity references), into a tree. Everything else is an
//! error: a document type declaration, an entity it does not know, a tag
//! that does not close, a second root. An error is never a guess.
//!
//! An element's name is kept without its namespace prefix, as the answers
//! use none; an attribute's name is kept whole (`xsi:nil`).

use std::fmt;

/// How deep elements may nest. The answers nest three deep; a document that
/// nests deeper than this is not one of them, and the limit keeps the
/// recursion from exhausting the stack.
const MAX_DEPTH: usize = 32;

/// One element of a document.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Element {
    /// The name, without a namespace prefix.
    pub name: String,
    /// The attributes in the order written, with their values decoded.
    pub attributes: Vec<(String, String)>,
    /// The child elements in the order written.
    pub children: Vec<Element>,
    /// The element's own text, decoded, with the text of CDATA sections.
    pub text: String,
}

impl Element {
    /// The first child of that name.
    pub fn child(&self, name: &str) -> Option<&Element> {
        self.children.iter().find(|child| child.name == name)
    }

    /// The children of that name.
    pub fn children_named<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Element> {
        self.children.iter().filter(move |child| child.name == name)
    }

    /// The value of an attribute.
    pub fn attribute(&self, name: &str) -> Option<&str> {
        self.attributes
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }

    /// The text of the first child of that name, trimmed. `None` when there
    /// is no such child; empty when it is empty.
    pub fn text_of(&self, name: &str) -> Option<&str> {
        self.child(name).map(|child| child.text.trim())
    }
}

/// Why a document could not be read, and where.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct XmlError {
    /// The byte at which the reader stopped.
    pub at: usize,
    /// What it found there.
    pub reason: String,
}

impl fmt::Display for XmlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} (at byte {})", self.reason, self.at)
    }
}

/// Reads a whole document into its root element.
pub(crate) fn parse(document: &str) -> Result<Element, XmlError> {
    let mut reader = Reader {
        text: document,
        at: 0,
    };
    reader.eat("\u{feff}");
    reader.skip_misc()?;
    if reader.rest().starts_with("<!") {
        return Err(reader.error("a document type declaration is not read"));
    }
    if !reader.rest().starts_with('<') {
        return Err(reader.error("the document does not begin with an element"));
    }
    let root = reader.element(0)?;
    reader.skip_misc()?;
    if !reader.rest().is_empty() {
        return Err(reader.error("there is more after the root element"));
    }
    Ok(root)
}

struct Reader<'a> {
    text: &'a str,
    at: usize,
}

impl<'a> Reader<'a> {
    fn rest(&self) -> &'a str {
        &self.text[self.at..]
    }

    fn error(&self, reason: impl Into<String>) -> XmlError {
        XmlError {
            at: self.at,
            reason: reason.into(),
        }
    }

    /// Steps over `prefix` if the rest begins with it.
    fn eat(&mut self, prefix: &str) -> bool {
        if self.rest().starts_with(prefix) {
            self.at += prefix.len();
            true
        } else {
            false
        }
    }

    fn skip_whitespace(&mut self) {
        let rest = self.rest();
        let trimmed = rest.trim_start_matches([' ', '\t', '\r', '\n']);
        self.at += rest.len() - trimmed.len();
    }

    /// Steps past the next `end`, returning what lay before it.
    fn until(&mut self, end: &str, what: &str) -> Result<&'a str, XmlError> {
        let Some(offset) = self.rest().find(end) else {
            return Err(self.error(format!("{what} does not end")));
        };
        let start = self.at;
        self.at += offset + end.len();
        Ok(&self.text[start..start + offset])
    }

    /// Steps over whitespace, comments and processing instructions, which
    /// may stand before and after the root element.
    fn skip_misc(&mut self) -> Result<(), XmlError> {
        loop {
            self.skip_whitespace();
            if self.eat("<?") {
                self.until("?>", "a processing instruction")?;
            } else if self.eat("<!--") {
                self.until("-->", "a comment")?;
            } else {
                return Ok(());
            }
        }
    }

    fn name(&mut self) -> Result<&'a str, XmlError> {
        let rest = self.rest();
        let length = rest
            .find(|c: char| !(c.is_alphanumeric() || matches!(c, '_' | '-' | '.' | ':')))
            .unwrap_or(rest.len());
        let name = &rest[..length];
        if name.is_empty() || name.starts_with(|c: char| c.is_ascii_digit() || c == '-' || c == '.')
        {
            return Err(self.error("a name was expected"));
        }
        self.at += length;
        Ok(name)
    }

    /// Reads an element whose `<` is next.
    fn element(&mut self, depth: usize) -> Result<Element, XmlError> {
        if depth >= MAX_DEPTH {
            return Err(self.error(format!("elements nest deeper than {MAX_DEPTH}")));
        }
        let open_at = self.at;
        if !self.eat("<") {
            return Err(self.error("an element was expected"));
        }
        let full_name = self.name()?.to_owned();
        let mut element = Element {
            name: local(&full_name).to_owned(),
            ..Element::default()
        };

        loop {
            let before = self.at;
            self.skip_whitespace();
            if self.eat("/>") {
                return Ok(element);
            }
            if self.eat(">") {
                break;
            }
            if self.at == before {
                return Err(self.error(format!("<{full_name}> is not closed")));
            }
            let key = self.name()?.to_owned();
            self.skip_whitespace();
            if !self.eat("=") {
                return Err(self.error(format!("the attribute {key} has no value")));
            }
            self.skip_whitespace();
            let quote = match self.rest().chars().next() {
                Some(quote @ ('"' | '\'')) => quote,
                _ => return Err(self.error(format!("the value of {key} is not in quotes"))),
            };
            self.at += 1;
            let value_at = self.at;
            let raw = self.until(&quote.to_string(), "an attribute's value")?;
            if raw.contains('<') {
                return Err(XmlError {
                    at: value_at,
                    reason: format!("the value of {key} holds a '<'"),
                });
            }
            let value = decode(raw).map_err(|reason| XmlError {
                at: value_at,
                reason,
            })?;
            if element.attribute(&key).is_some() {
                return Err(self.error(format!("the attribute {key} is written twice")));
            }
            element.attributes.push((key, value));
        }

        loop {
            let text_at = self.at;
            let Some(offset) = self.rest().find('<') else {
                return Err(XmlError {
                    at: open_at,
                    reason: format!("the document ends inside <{full_name}>"),
                });
            };
            let raw = &self.text[self.at..self.at + offset];
            if raw.contains("]]>") {
                return Err(self.error("text holds \"]]>\""));
            }
            let text = decode(raw).map_err(|reason| XmlError {
                at: text_at,
                reason,
            })?;
            element.text.push_str(&text);
            self.at += offset;

            if self.eat("</") {
                let close_at = self.at;
                let close = self.name()?;
                if close != full_name {
                    return Err(XmlError {
                        at: close_at,
                        reason: format!("<{full_name}> is closed by </{close}>"),
                    });
                }
                self.skip_whitespace();
                if !self.eat(">") {
                    return Err(self.error(format!("</{full_name}> is not closed")));
                }
                return Ok(element);
            } else if self.eat("<!--") {
                self.until("-->", "a comment")?;
            } else if self.eat("<![CDATA[") {
                let data = self.until("]]>", "a CDATA section")?;
                element.text.push_str(data);
            } else if self.eat("<?") {
                self.until("?>", "a processing instruction")?;
            } else if self.rest().starts_with("<!") {
                return Err(self.error("a declaration inside an element is not read"));
            } else {
                let child = self.element(depth + 1)?;
                element.children.push(child);
            }
        }
    }
}

/// A name without its namespace prefix.
fn local(name: &str) -> &str {
    name.rsplit_once(':').map_or(name, |(_, local)| local)
}

/// Replaces the entity and character references of a piece of text.
fn decode(raw: &str) -> Result<String, String> {
    if !raw.contains('&') {
        return Ok(raw.to_owned());
    }
    let mut decoded = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(amp) = rest.find('&') {
        decoded.push_str(&rest[..amp]);
        rest = &rest[amp + 1..];
        let Some(semicolon) = rest.find(';') else {
            return Err("an '&' that begins no reference".to_owned());
        };
        let reference = &rest[..semicolon];
        let character = match reference {
            "lt" => '<',
            "gt" => '>',
            "amp" => '&',
            "quot" => '"',
            "apos" => '\'',
            _ => {
                let code = if let Some(hex) = reference.strip_prefix("#x") {
                    u32::from_str_radix(hex, 16).ok()
                } else if let Some(decimal) = reference.strip_prefix('#') {
                    decimal.parse::<u32>().ok()
                } else {
                    return Err(format!("the entity &{reference}; is not known"));
                };
                code.filter(|&code| code != 0)
                    .and_then(char::from_u32)
                    .ok_or_else(|| format!("&{reference}; is no character"))?
            }
        };
        decoded.push(character);
        rest = &rest[semicolon + 1..];
    }
    decoded.push_str(rest);
    Ok(decoded)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fails(document: &str) -> XmlError {
        parse(document).expect_err(document)
    }

    #[test]
    fn reads_an_answer_of_the_service() {
        let document = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\r\n\
            <datiOggi xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\" \
            xmlns=\"http://www.meteotrentino.it/\">\r\n  <data>2026/11/22</data>\r\n  \
            <tmin xsi:nil=\"true\" />\r\n  <venti>\r\n    \
            <vento_al_suolo UM_VV=\"m/s\" UM_VVMAX=\"m/s\" UM_DV=\"gN\">\r\n      \
            <data>2026-11-21T00:00:00</data>\r\n      <v>1.9</v>\r\n    \
            </vento_al_suolo>\r\n  </venti>\r\n  <temperature />\r\n</datiOggi>";
        let root = parse(document).unwrap();
        assert_eq!(root.name, "datiOggi");
        assert_eq!(
            root.attribute("xmlns"),
            Some("http://www.meteotrentino.it/")
        );
        assert_eq!(root.text_of("data"), Some("2026/11/22"));
        assert_eq!(
            root.child("tmin").unwrap().attribute("xsi:nil"),
            Some("true")
        );
        assert_eq!(root.text_of("tmin"), Some(""));
        let wind = root.child("venti").unwrap();
        let rows: Vec<&Element> = wind.children_named("vento_al_suolo").collect();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].attribute("UM_DV"), Some("gN"));
        assert_eq!(rows[0].text_of("v"), Some("1.9"));
        assert_eq!(rows[0].text_of("vmax"), None);
        assert!(root.child("temperature").unwrap().children.is_empty());
        assert_eq!(root.children.len(), 4);
    }

    #[test]
    fn reads_the_rest_of_what_a_document_may_hold() {
        let root = parse(
            "\u{feff}<!-- before --><?pi x?>\n<a:root b:c='1' d = \"x &amp; y\">\
             one<!-- inside -->two<![CDATA[<three> & ]]>&lt;&gt;&quot;&apos;&#65;&#x42;&#xe9;\
             <?pi?><child/></a:root>\n<!-- after -->\n",
        )
        .unwrap();
        assert_eq!(root.name, "root");
        assert_eq!(root.attribute("b:c"), Some("1"));
        assert_eq!(root.attribute("d"), Some("x & y"));
        assert_eq!(root.text, "onetwo<three> & <>\"'ABé");
        assert_eq!(root.children.len(), 1);
        assert_eq!(root.children[0].name, "child");
    }

    #[test]
    fn keeps_the_text_of_names_as_written() {
        let root = parse("<a><nome>Male' (Bivacco Marinelli)</nome><b>  </b></a>").unwrap();
        assert_eq!(root.text_of("nome"), Some("Male' (Bivacco Marinelli)"));
        assert_eq!(root.text_of("b"), Some(""));
    }

    #[test]
    fn refuses_what_is_not_well_formed() {
        for (document, reason) in [
            ("", "the document does not begin with an element"),
            ("text", "the document does not begin with an element"),
            ("<a>", "the document ends inside <a>"),
            ("<a><b></a>", "<b> is closed by </a>"),
            ("<a></b>", "<a> is closed by </b>"),
            ("<a/><b/>", "there is more after the root element"),
            ("<a/>text", "there is more after the root element"),
            ("<a x=1/>", "the value of x is not in quotes"),
            ("<a x/>", "the attribute x has no value"),
            ("<a x='1' x='2'/>", "the attribute x is written twice"),
            ("<a x='1/>", "an attribute's value does not end"),
            ("<a x='<'/>", "the value of x holds a '<'"),
            ("<a>&nbsp;</a>", "the entity &nbsp; is not known"),
            ("<a>a & b</a>", "an '&' that begins no reference"),
            ("<a>&#0;</a>", "&#0; is no character"),
            ("<a>&#xD800;</a>", "&#xD800; is no character"),
            ("<a>&#x;</a>", "&#x; is no character"),
            ("<a>]]></a>", "text holds \"]]>\""),
            ("<a><!-- </a>", "a comment does not end"),
            ("<a><![CDATA[ </a>", "a CDATA section does not end"),
            (
                "<!DOCTYPE a><a/>",
                "a document type declaration is not read",
            ),
            (
                "<a><!ENTITY x 'y'></a>",
                "a declaration inside an element is not read",
            ),
            ("<1a/>", "a name was expected"),
            ("< a/>", "a name was expected"),
            ("<a></ >", "a name was expected"),
            ("<a></a", "</a> is not closed"),
            ("<a b='1'c='2'/>", "<a> is not closed"),
            (
                "<?xml version='1.0'",
                "a processing instruction does not end",
            ),
        ] {
            assert_eq!(fails(document).reason, reason, "{document:?}");
        }
    }

    #[test]
    fn says_where_it_stopped() {
        let error = fails("<a>\n  <b></c>\n</a>");
        assert_eq!(error.at, 11);
        assert_eq!(error.to_string(), "<b> is closed by </c> (at byte 11)");
    }

    #[test]
    fn stops_at_a_depth_no_answer_has() {
        let deep = |n: usize| format!("{}{}", "<a>".repeat(n), "</a>".repeat(n));
        assert!(parse(&deep(MAX_DEPTH)).is_ok());
        assert_eq!(
            fails(&deep(MAX_DEPTH + 1)).reason,
            format!("elements nest deeper than {MAX_DEPTH}")
        );
        // Far deeper than the stack would allow without the limit.
        assert!(parse(&deep(100_000)).is_err());
    }

    #[test]
    fn never_panics_on_a_cut_document() {
        let document =
            "<?xml version=\"1.0\"?><r x=\"&amp;é\"><a>1</a><b/><!--c--><![CDATA[d]]>é</r>";
        for end in 0..document.len() {
            if document.is_char_boundary(end) {
                let _ = parse(&document[..end]);
            }
        }
        assert!(parse(document).is_ok());
    }
}
