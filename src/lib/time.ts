const MINUTE = 60;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;

// "just now", "5 min ago", "3 days ago", or a date for anything older than a month.
export function relativeTime(unixSeconds: number | null, now: number = Date.now() / 1000): string {
	if (unixSeconds === null) {
		return "Never";
	}

	const elapsed = Math.max(0, now - unixSeconds);
	if (elapsed < MINUTE) {
		return "Just now";
	}
	else if (elapsed < HOUR) {
		return `${Math.floor(elapsed / MINUTE)} min ago`;
	}
	else if (elapsed < DAY) {
		const hours = Math.floor(elapsed / HOUR);
		return hours === 1 ? "1 hour ago" : `${hours} hours ago`;
	}
	else if (elapsed < 30 * DAY) {
		const days = Math.floor(elapsed / DAY);
		return days === 1 ? "Yesterday" : `${days} days ago`;
	}

	return new Date(unixSeconds * 1000).toLocaleDateString("en-GB", {
		day: "numeric",
		month: "short",
		year: "numeric",
	});
}

// Connection counts for each of the last `days` days, oldest first.
export function dailyCounts(
	startTimes: number[],
	days: number,
	now: Date = new Date(),
): Array<{ day: Date; count: number }> {
	const today = new Date(now.getFullYear(), now.getMonth(), now.getDate());
	const buckets = Array.from({ length: days }, (_, index) => {
		const day = new Date(today);
		day.setDate(today.getDate() - (days - 1 - index));
		return { day, count: 0 };
	});

	for (const time of startTimes) {
		const date = new Date(time * 1000);
		const start = new Date(date.getFullYear(), date.getMonth(), date.getDate());
		const offset = Math.round((today.getTime() - start.getTime()) / (DAY * 1000));
		if (offset >= 0 && offset < days) {
			buckets[days - 1 - offset].count += 1;
		}
	}

	return buckets;
}
