// Adapted from SemplificaAI/gliner2-rs (Apache-2.0):
// https://github.com/SemplificaAI/gliner2-rs/blob/main/rust_component/src/processor.rs
// Original: Copyright 2026 Dario Finardi, Semplifica s.r.l.
//
// Modifications: char offsets (anno convention) instead of token offsets;
// integration with anno::Entity / anno::backends::inference traits;
// removal of Relations and Classifications schema arms (NER-only Phase 1).
// Error type translated from anyhow::Result to backend-local Error.

use crate::backends::gliner2_fastino::errors::Error;
use tokenizers::Tokenizer;

pub const P_TOKEN: &str = "[P]";
pub const E_TOKEN: &str = "[E]";
pub const C_TOKEN: &str = "[C]";
pub const L_TOKEN: &str = "[L]";
pub const R_TOKEN: &str = "[R]";
pub const SEP_STRUCT: &str = "[SEP_STRUCT]";
pub const SEP_TEXT: &str = "[SEP_TEXT]";

/// Integer IDs for the seven fastino special tokens, resolved at load time
/// from the tokenizer's vocabulary. Never hardcoded.
#[derive(Debug, Clone)]
pub struct SpecialTokenIds {
    pub p: u32,
    pub e: u32,
    pub c: u32,
    pub l: u32,
    pub r: u32,
    pub sep_struct: u32,
    pub sep_text: u32,
}

impl SpecialTokenIds {
    pub fn resolve(tokenizer: &Tokenizer) -> Result<Self, Error> {
        let lookup = |tok: &'static str| -> Result<u32, Error> {
            tokenizer
                .token_to_id(tok)
                .ok_or(Error::SpecialTokenMissing { token: tok })
        };
        Ok(Self {
            p: lookup(P_TOKEN)?,
            e: lookup(E_TOKEN)?,
            c: lookup(C_TOKEN)?,
            l: lookup(L_TOKEN)?,
            r: lookup(R_TOKEN)?,
            sep_struct: lookup(SEP_STRUCT)?,
            sep_text: lookup(SEP_TEXT)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stub_tokenizer() -> Tokenizer {
        Tokenizer::from_file("testdata/gliner2_fastino/stub_tokenizer.json")
            .expect("stub fixture missing or invalid")
    }

    #[test]
    fn resolve_special_tokens_from_stub_fixture() {
        let tok = stub_tokenizer();
        let ids = SpecialTokenIds::resolve(&tok).unwrap();
        assert_eq!(ids.p, 2);
        assert_eq!(ids.e, 3);
        assert_eq!(ids.c, 4);
        assert_eq!(ids.l, 5);
        assert_eq!(ids.r, 6);
        assert_eq!(ids.sep_struct, 7);
        assert_eq!(ids.sep_text, 8);
    }

    #[test]
    fn missing_special_token_returns_typed_error() {
        // Build a tokenizer.json missing [SEP_TEXT]
        let mut content = std::fs::read_to_string("testdata/gliner2_fastino/stub_tokenizer.json").unwrap();
        content = content.replace("\"[SEP_TEXT]\"", "\"[NOT_THE_TOKEN]\"");
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), &content).unwrap();
        let tok = Tokenizer::from_file(tmp.path()).unwrap();

        let err = SpecialTokenIds::resolve(&tok).unwrap_err();
        match err {
            Error::SpecialTokenMissing { token } => assert_eq!(token, SEP_TEXT),
            other => panic!("expected SpecialTokenMissing, got {other:?}"),
        }
    }
}
