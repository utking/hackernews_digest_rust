-- Your SQL goes here
CREATE TABLE `rss_items`(
	`id` BIGINT NOT NULL,
	`source` TEXT NOT NULL,
	`created_at` BIGINT NOT NULL,
	PRIMARY KEY(`id`, `source`)
);
