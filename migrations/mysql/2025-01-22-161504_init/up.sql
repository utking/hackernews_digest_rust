-- Your SQL goes here
CREATE TABLE `news_items`(
	`id` INTEGER NOT NULL PRIMARY KEY,
	`news_title` VARCHAR(512) NOT NULL,
	`news_url` VARCHAR(1024) NOT NULL,
	`created_at` INTEGER NOT NULL
);
