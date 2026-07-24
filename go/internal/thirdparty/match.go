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
	EntityType string   // optional: 'player' | 'team'; enables team-specific alias handling
	Name       string   // canonical display name (e.g., "LeBron James", "FC Bayern Munich")
	FirstName  string   // used for "first + last" multi-part matching (players)
	LastName   string   // used for "first + last" multi-part matching (players)
	Team       string   // optional team context — enables "name part + team" match for players
	Aliases    []string // alternate name forms from search_aliases
	Sport      string   // used to resolve sport-context terms for short aliases
}

// MatchesEntity reports whether the supplied text mentions the given entity.
// The logic is shared by news article filtering and downstream corpus stages,
// so behavior stays consistent across the news rail.
func MatchesEntity(text string, in EntityMatchInput) bool {
	if nameInText(
		in.Name,
		text,
		in.FirstName,
		in.LastName,
		in.Team,
		in.Aliases,
		SportContextTerms(in.Sport),
	) {
		return true
	}
	return isTeamEntity(in.EntityType) && trustedShortTeamAliasPos(text, in.Aliases) >= 0
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

// FirstMatchPos reports the earliest character index in text at which the entity
// is mentioned (by name, alias, or first+last match), or -1 if absent. It accepts
// exactly what MatchesEntity accepts, but reports WHERE the match starts.
//
// It exists for co-mention precision: recording each entity's position in an
// article title lets consumers require two entities to appear in PROXIMITY (the
// same phrase) rather than merely both somewhere in a flat multi-subject headline
// — e.g. "Everton want Chelsea's Delap and Spurs' Gallagher" should NOT link
// Chelsea↔Gallagher (66 chars apart) the way bare co-occurrence does.
func FirstMatchPos(text string, in EntityMatchInput) int {
	pos := firstMatchPos(
		in.Name, text, in.FirstName, in.LastName, in.Team, in.Aliases,
		SportContextTerms(in.Sport),
	)
	if isTeamEntity(in.EntityType) {
		if aliasPos := trustedShortTeamAliasPos(text, in.Aliases); aliasPos >= 0 {
			return earliest(pos, aliasPos)
		}
	}
	return pos
}

// firstMatchPos mirrors nameInText's acceptance logic but returns the earliest
// matching index instead of a bool. A returned >= 0 is exactly equivalent to
// nameInText returning true.
func firstMatchPos(name, text, firstName, lastName, team string, aliases []string, sportContext string) int {
	if name == "" || text == "" {
		return -1
	}
	nameLower := strings.ToLower(strings.TrimSpace(name))
	textLower := strings.ToLower(strings.TrimSpace(text))

	best := -1
	consider := func(idx int) {
		if idx >= 0 && (best == -1 || idx < best) {
			best = idx
		}
	}

	consider(strings.Index(textLower, nameLower))

	for _, alias := range aliases {
		aliasLower := strings.ToLower(strings.TrimSpace(alias))
		if aliasLower == "" {
			continue
		}
		if len(alias) < 4 {
			if sportContext != "" && strings.Contains(textLower, strings.ToLower(strings.TrimSpace(sportContext))) {
				consider(wordBoundaryIndex(aliasLower, textLower))
			}
			continue
		}
		consider(strings.Index(textLower, aliasLower))
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
		fnIdx, lnIdx := -1, -1
		if len(fn) > 1 {
			fnIdx = wordBoundaryIndex(fn, textLower)
		}
		if len(ln) > 1 {
			lnIdx = wordBoundaryIndex(ln, textLower)
		}

		if fnIdx >= 0 && lnIdx >= 0 {
			consider(earliest(fnIdx, lnIdx))
		}
		if team != "" && (fnIdx >= 0 || lnIdx >= 0) {
			if strings.Contains(textLower, strings.ToLower(strings.TrimSpace(team))) {
				consider(earliest(fnIdx, lnIdx)) // the player's span anchors the position
			}
		}
	}

	return best
}

// earliest returns the smaller non-negative of a, b (or -1 if both are negative).
func earliest(a, b int) int {
	switch {
	case a < 0:
		return b
	case b < 0:
		return a
	case a < b:
		return a
	default:
		return b
	}
}

// wordBoundaryIndex returns the first index of a whole-word match of word in
// text, or -1. The boundary-anchored sibling of wordBoundaryMatch.
func wordBoundaryIndex(word, text string) int {
	re, err := regexp.Compile(`\b` + regexp.QuoteMeta(word) + `\b`)
	if err != nil {
		if i := strings.Index(text, word); i >= 0 {
			return i
		}
		return -1
	}
	loc := re.FindStringIndex(text)
	if loc == nil {
		return -1
	}
	return loc[0]
}

func trustedShortTeamAliasPos(text string, aliases []string) int {
	textLower := strings.ToLower(strings.TrimSpace(text))
	if textLower == "" {
		return -1
	}

	best := -1
	consider := func(idx int) {
		if idx >= 0 && (best == -1 || idx < best) {
			best = idx
		}
	}
	for _, alias := range aliases {
		alias = strings.TrimSpace(alias)
		if alias == "" || !trustedShortTeamAlias(alias) {
			continue
		}
		consider(wordBoundaryIndex(strings.ToLower(alias), textLower))
	}
	return best
}

func isTeamEntity(entityType string) bool {
	return strings.EqualFold(strings.TrimSpace(entityType), "team")
}
