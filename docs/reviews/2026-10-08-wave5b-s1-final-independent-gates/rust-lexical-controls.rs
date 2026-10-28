extern crate self as sqlx;
#[macro_export]
macro_rules! migrate { ($path:literal) => { $path }; }
mod raw_alias {
    use sqlx::migrate as r#embed;
    pub const VALUE: &str = r#embed!("../../migrations");
}
mod raw_import {
    use sqlx::r#migrate as embed;
    pub const VALUE: &str = embed!("../../migrations");
}
mod quote_char {
    const QUOTE: char = '"';
    use sqlx::migrate as embed;
    pub const VALUE: &str = embed!("../../migrations");
}
