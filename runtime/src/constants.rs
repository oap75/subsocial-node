pub mod currency {
	use subsocial_primitives::Balance;

	pub const UNITS: Balance = 10_000_000_000;
	pub const DOLLARS: Balance = UNITS;            // 10_000_000_000
	pub const CENTS: Balance = DOLLARS / 100;      // 100_000_000
	pub const MILLICENTS: Balance = CENTS / 1_000; // 100_000

	pub const fn deposit(items: u32, bytes: u32) -> Balance {
		items as Balance * 20 * DOLLARS + (bytes as Balance) * 100 * MILLICENTS
	}
}

pub mod time {
	use subsocial_primitives::{Moment, BlockNumber};

	pub const MILLISECS_PER_BLOCK: Moment = 6000;
	pub const SLOT_DURATION: Moment = MILLISECS_PER_BLOCK;

	// These time units are defined in number of blocks.
	pub const MINUTES: BlockNumber = 60_000 / (MILLISECS_PER_BLOCK as BlockNumber);
	pub const HOURS: BlockNumber = MINUTES * 60;
	pub const DAYS: BlockNumber = HOURS * 24;
}
