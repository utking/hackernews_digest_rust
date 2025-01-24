-- Your SQL goes here
CREATE TABLE `rss_items`(
	`id` INTEGER NOT NULL,
	`source` TEXT NOT NULL,
	`created_at` INTEGER NOT NULL,
	PRIMARY KEY(`id`, `source`)
);

