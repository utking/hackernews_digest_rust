// @generated automatically by Diesel CLI.

diesel::table! {
    news_items (id) {
        id -> Integer,
        news_title -> Text,
        news_url -> Text,
        created_at -> Integer,
    }
}
