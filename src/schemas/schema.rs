// @generated automatically by Diesel CLI.

diesel::table! {
    rss_items (id, source) {
        id -> BigInt,
        source -> VarChar,
        created_at -> BigInt,
    }
}
