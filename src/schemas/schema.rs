// @generated automatically by Diesel CLI.

diesel::table! {
    news_items (id) {
        id -> Integer,
        news_title -> Text,
        news_url -> Text,
        created_at -> Integer,
    }
}

diesel::table! {
    rss_items (id, source) {
        id -> Integer,
        source -> Text,
        created_at -> Integer,
    }
}

diesel::allow_tables_to_appear_in_same_query!(
    news_items,
    rss_items,
);
