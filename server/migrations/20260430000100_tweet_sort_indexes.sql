CREATE INDEX idx_tweet_created_at_id
ON tweet.tweet (created_at DESC, id DESC);

CREATE INDEX idx_tweet_author_created_at_id
ON tweet.tweet (author_id, created_at DESC, id DESC);

CREATE INDEX idx_tweet_updated_at_id
ON tweet.tweet (updated_at DESC, id DESC);

CREATE INDEX idx_tweet_author_updated_at_id
ON tweet.tweet (author_id, updated_at DESC, id DESC);
