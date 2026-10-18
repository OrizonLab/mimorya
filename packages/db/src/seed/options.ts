export interface SeedOptions {
  /** Argon2id hash applied to every demo account (null: accounts cannot log in). */
  passwordHash: string | null;
  reset?: boolean;
}
