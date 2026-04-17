package thirdparty

import (
	"regexp"
	"strings"
)

// SportContextTerms returns the phrase we expect to co-occur with short aliases.
// Used by MatchesEntity to disambiguate 2–3-letter aliases (e.g. team abbreviations).
func SportContextTerms(sport string) string {
	return sportTerms[strings.ToUpper(strings.TrimSpace(sport))]
}

// EntityMatchInput describes the entity we're matching against text. All fields
// besides Name are optional; richer inputs produce stricter, more accurate matches.
type EntityMatchInput struct {
	Name      string   // canonical display name (e.g., "LeBron James", "FC Bayern Munich")
	FirstName string   // used for "first + last" multi-part matching (players)
	LastName  string   // used for "first + last" multi-part matching (players)
	Team      string   // optional team context — enables "name part + team" match for players
	Aliases   []string // alternate name forms from search_aliases
	Sport     string   // used to resolve sport-context terms for short aliases
}

// MatchesEntity reports whether the supplied text mentions the given entity.
// The logic is shared between news article filtering and tweet entity linking,
// so behaviour stays consistent across integrations (see architecture notes).
func MatchesEntity(text string, in EntityMatchInput) bool {
	return nameInText(
		in.Name,
		text,
		in.FirstName,
		in.LastName,
		in.Team,
		in.Aliases,
		SportContextTerms(in.Sport),
	)
}

// nameInText is the concrete matcher. It mirrors the previous news-only logic.
// Short aliases (<4 chars) only match when the text also contains a sport-context
// term, which keeps ambiguous tokens like "LA" from matching unrelated articles.
func nameInText(name, text, firstName, lastName, team string, aliases []string, sportContext string) bool {
	if name == "" || text == "" {
		return false
	}
	nameLower := strings.ToLower(strings.TrimSpace(name))
	textLower := strings.ToLower(strings.TrimSpace(text))

	if strings.Contains(textLower, nameLower) {
		return true
	}

	for _, alias := range aliases {
		aliasLower := strings.ToLower(strings.TrimSpace(alias))
		if aliasLower == "" {
			continue
		}
		if len(alias) < 4 {
			if sportContext != "" && strings.Contains(textLower, strings.ToLower(strings.TrimSpace(sportContext))) {
				if wordBoundaryMatch(aliasLower, textLower) {
					return true
				}
			}
			continue
		}
		if strings.Contains(textLower, aliasLower) {
			return true
		}
	}

	nameParts := strings.Fields(nameLower)
	if len(nameParts) >= 2 {
		fn := strings.ToLower(strings.TrimSpace(firstName))
		if fn == "" {
			fn = nameParts[0]
		}
		ln := strings.ToLower(strings.TrimSpace(lastName))
		if ln == "" {
			ln = nameParts[len(nameParts)-1]
		}

		fnMatch := len(fn) > 1 && wordBoundaryMatch(fn, textLower)
		lnMatch := len(ln) > 1 && wordBoundaryMatch(ln, textLower)

		if fnMatch && lnMatch {
			return true
		}

		if team != "" && (fnMatch || lnMatch) {
			if strings.Contains(textLower, strings.ToLower(strings.TrimSpace(team))) {
				return true
			}
		}
	}

	return false
}

func wordBoundaryMatch(word, text string) bool {
	re, err := regexp.Compile(`\b` + regexp.QuoteMeta(word) + `\b`)
	if err != nil {
		return strings.Contains(text, word)
	}
	return re.MatchString(text)
}
