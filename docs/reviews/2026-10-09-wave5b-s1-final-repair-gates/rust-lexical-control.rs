// Lexical control for the review N3/N4 fixture spellings: every form the embed
// scanner is asked to read is legal Rust. Compiled with rustc, edition 2021.
#![allow(unused, dead_code, non_snake_case)]

mod sqlx {
    macro_rules! r#migrate {
        ($s:expr) => {
            $s
        };
    }
    pub(crate) use r#migrate;
    pub mod r#type {
        pub fn migrate() {}
    }
}

macro_rules! embed {
    ($s:expr) => {
        $s
    };
}
macro_rules! r#rawembed {
    ($s:expr) => {
        $s
    };
}

// N3: raw identifiers as the bound name, the imported segment and the call.
use sqlx::r#migrate as r#alias;
use sqlx::r#type::migrate as path_segment;
static DIRECT: &str = sqlx::r#migrate!("../../migrations");
static RAW_CALL: &str = r#rawembed!("../../migrations");
static ALIASED: &str = r#alias!("../../migrations");

// N4: the literal and lifetime forms the unified lexer has to tell apart.
const QUOTE: char = '"';
const APOS: char = '\'';
const BSLASH: char = '\\';
const SMILE: char = '\u{1F600}';
const HEX: char = '\x41';
const BYTE: u8 = b'\n';
const BYTE_CHAR: u8 = b'x';
const RAW_STR: &str = r#"sqlx::migrate!("not an embed")"#;
const RAW_STR2: &str = r##"sqlx::migrate!("not an embed")"##;
const BYTE_STR: &[u8] = b"sqlx::migrate!(\"not an embed\")";
const BYTE_RAW: &[u8] = br#"sqlx::migrate!("not an embed")"#;
fn takes<'a, 'b>(x: &'a str, _y: &'b str, _z: &'static str, _dyn_: &'_ str) -> &'a str { x }

// A raw identifier is a name, never a keyword.
fn raw_names() {
    let r#use = 1;
    let r#type = 2;
    let r#struct = ("fn", "if");
    let _ = (r#use, r#type, r#struct, path_segment, RAW_STR, RAW_STR2, BYTE_STR, BYTE_RAW);
}