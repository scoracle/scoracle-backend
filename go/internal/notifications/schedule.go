package notifications

import (
	"math/rand/v2"
	"time"
)

// ScheduleDelivery picks a random time within [matchTime, matchTime+window]
// that falls inside waking hours (9 AM – 10 PM) in the user's timezone.
func ScheduleDelivery(matchTime time.Time, timezone string) time.Time {
	loc, err := time.LoadLocation(timezone)
	if err != nil {
		loc = time.UTC
	}

	windowEnd := matchTime.Add(time.Duration(defaultWindowHours) * time.Hour)
	window := windowEnd.Sub(matchTime)

	for i := 0; i < maxScheduleRetries; i++ {
		offset := time.Duration(rand.Int64N(int64(window)))
		candidate := matchTime.Add(offset).In(loc)
		if isWakingHour(candidate.Hour()) {
			return candidate
		}
	}

	// Fallback: 9 AM + random minute next day in user's timezone
	next := matchTime.Add(24 * time.Hour).In(loc)
	return time.Date(next.Year(), next.Month(), next.Day(),
		quietEndHour, rand.IntN(60), 0, 0, loc)
}

func isWakingHour(hour int) bool {
	return hour >= quietEndHour && hour < quietStartHour
}
