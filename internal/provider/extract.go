package provider

import "strconv"

// ExtractValue normalizes a stat value from various API response formats.
//
// SportMonks returns dicts like {"total": 15, "goals": 12, "penalties": 3}.
// BDL returns flat numbers. This handles both, extracting the aggregate.
//
// Returns the scalar float64 value, and ok=false if not extractable.
func ExtractValue(val interface{}) (float64, bool) {
	if val == nil {
		return 0, false
	}

	switch v := val.(type) {
	case float64:
		return v, true
	case int:
		return float64(v), true
	case int64:
		return float64(v), true
	case string:
		if f, err := strconv.ParseFloat(v, 64); err == nil {
			return f, true
		}
		return 0, false
	case map[string]interface{}:
		// SportMonks nested objects: try "total", "all", "count", "average"
		for _, key := range []string{"total", "all", "count", "average"} {
			if inner, exists := v[key]; exists && inner != nil {
				return ExtractValue(inner)
			}
		}
		return 0, false
	default:
		return 0, false
	}
}
