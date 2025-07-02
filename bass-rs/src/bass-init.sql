PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS `music` (
  `id` integer PRIMARY KEY,
  `title` string NOT NULL,
  `source` string NOT NULL,
  `composer` string,
  `arranger` string,
  `notes` string,
  `runtime` integer
);

CREATE TABLE IF NOT EXISTS `music_keywords` (
  `mid` integer NOT NULL,
  `kid` integer NOT NULL,
  PRIMARY KEY (`mid`, `kid`),
  FOREIGN KEY(mid) REFERENCES music(id) ON DELETE CASCADE,
  FOREIGN KEY(kid) REFERENCES keywords(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS `keywords` (
  `id` integer PRIMARY KEY,
  `category` string,
  `keyword` string NOT NULL
);
