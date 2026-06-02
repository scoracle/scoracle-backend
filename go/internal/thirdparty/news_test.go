package thirdparty

import "testing"

func TestNameInText_ExactMatch(t *testing.T) {
	if !nameInText("FC Bayern Munchen", "FC Bayern Munchen wins again", "", "", "", nil, "") {
		t.Error("expected exact match")
	}
}

func TestNameInText_CaseInsensitive(t *testing.T) {
	if !nameInText("FC Bayern Munchen", "fc bayern munchen wins again", "", "", "", nil, "") {
		t.Error("expected case-insensitive match")
	}
}

func TestNameInText_AliasMatch(t *testing.T) {
	aliases := []string{"Bayern Munich", "Bayern München", "FC Bayern", "Bayern"}

	if !nameInText("FC Bayern Munchen", "Bayern Munich transfer news", "", "", "", aliases, "") {
		t.Error("expected alias 'Bayern Munich' to match")
	}

	if !nameInText("FC Bayern Munchen", "Bayern München beats rival", "", "", "", aliases, "") {
		t.Error("expected alias 'Bayern München' to match")
	}

	if !nameInText("FC Bayern Munchen", "FC Bayern signs striker", "", "", "", aliases, "") {
		t.Error("expected alias 'FC Bayern' to match")
	}
}

func TestNameInText_ShortAliasRequiresSportContext(t *testing.T) {
	aliases := []string{"FCB"}

	// Short alias without sport context — should NOT match.
	if nameInText("FC Barcelona", "FCB releases new product", "", "", "", aliases, "") {
		t.Error("short alias without sport context should not match")
	}

	// Short alias WITH sport context in text — should match.
	if !nameInText("FC Barcelona", "FCB soccer football update", "", "", "", aliases, "soccer football") {
		t.Error("short alias with sport context should match")
	}
}

func TestNameInText_NoFalsePositive(t *testing.T) {
	aliases := []string{"Bayern Munich"}

	if nameInText("FC Bayern Munchen", "Real Madrid wins championship", "", "", "", aliases, "") {
		t.Error("should not match unrelated text")
	}
}

func TestNameInText_PlayerWithParts(t *testing.T) {
	if !nameInText("LeBron James", "LeBron and James dominate", "LeBron", "James", "Lakers", nil, "") {
		t.Error("expected first+last name match")
	}
}

func TestNameInText_PlayerPartWithTeam(t *testing.T) {
	if !nameInText("LeBron James", "James leads the Lakers", "LeBron", "James", "Lakers", nil, "") {
		t.Error("expected last name + team context match")
	}
}

func TestBestAliasQuery(t *testing.T) {
	tests := []struct {
		primary string
		aliases []string
		want    string
	}{
		{"FC Bayern Munchen", []string{"Bayern Munich", "FC Bayern", "BAY"}, "Bayern Munich"},
		{"FC Bayern Munchen", []string{"FCB"}, ""},       // too short
		{"Bayern Munich", []string{"bayern munich"}, ""}, // same as primary
		{"Arsenal", nil, ""}, // no aliases
		{"Arsenal", []string{"Arsenal FC", "The Gunners"}, "The Gunners"}, // longest wins
	}

	for _, tt := range tests {
		got := bestAliasQuery(tt.primary, tt.aliases)
		if got != tt.want {
			t.Errorf("bestAliasQuery(%q, %v) = %q, want %q", tt.primary, tt.aliases, got, tt.want)
		}
	}
}

func TestBuildSearchName(t *testing.T) {
	tests := []struct {
		full, first, last, want string
	}{
		{"Neymar da Silva Santos Junior", "Neymar", "Junior", "Neymar Junior"},
		{"LeBron James", "LeBron", "James", "LeBron James"},
		{"Robert Lewandowski", "Robert", "Lewandowski", "Robert Lewandowski"},
		{"Saquon Barkley Jr", "", "", "Saquon Jr"},
	}

	for _, tt := range tests {
		got := buildSearchName(tt.full, tt.first, tt.last)
		if got != tt.want {
			t.Errorf("buildSearchName(%q, %q, %q) = %q, want %q", tt.full, tt.first, tt.last, got, tt.want)
		}
	}
}

// --- FirstMatchPos / proximity gate -----------------------------------------

func TestFirstMatchPos_BasicAndAbsent(t *testing.T) {
	title := "Everton want Chelsea striker Liam Delap and Tottenham Hotspur midfielder Conor Gallagher"

	chelsea := EntityMatchInput{Name: "Chelsea", Sport: "FOOTBALL"}
	if got := FirstMatchPos(title, chelsea); got != 13 {
		t.Errorf("Chelsea pos = %d, want 13", got)
	}
	gallagher := EntityMatchInput{Name: "Conor Gallagher", FirstName: "Conor", LastName: "Gallagher", Sport: "FOOTBALL"}
	if got := FirstMatchPos(title, gallagher); got != 73 {
		t.Errorf("Gallagher pos = %d, want 73", got)
	}
	absent := EntityMatchInput{Name: "Mohamed Salah", FirstName: "Mohamed", LastName: "Salah", Sport: "FOOTBALL"}
	if got := FirstMatchPos(title, absent); got != -1 {
		t.Errorf("absent entity pos = %d, want -1", got)
	}
}

// The whole point: the proximity gate keeps the genuine pair and drops the
// roundup artifact in the same headline.
func TestFirstMatchPos_ProximityGate(t *testing.T) {
	title := "Everton want Chelsea striker Liam Delap and Tottenham Hotspur midfielder Conor Gallagher"
	const window = 50

	chelsea := FirstMatchPos(title, EntityMatchInput{Name: "Chelsea", Sport: "FOOTBALL"})
	delap := FirstMatchPos(title, EntityMatchInput{Name: "Liam Delap", FirstName: "Liam", LastName: "Delap", Sport: "FOOTBALL"})
	gallagher := FirstMatchPos(title, EntityMatchInput{Name: "Conor Gallagher", FirstName: "Conor", LastName: "Gallagher", Sport: "FOOTBALL"})

	if abs(chelsea-delap) > window {
		t.Errorf("Chelsea↔Delap (%d) should be within window", abs(chelsea-delap))
	}
	if abs(chelsea-gallagher) <= window {
		t.Errorf("Chelsea↔Gallagher (%d) should be OUTSIDE window (spurious)", abs(chelsea-gallagher))
	}
}

// FirstMatchPos must agree with MatchesEntity on presence/absence.
func TestFirstMatchPos_AgreesWithMatchesEntity(t *testing.T) {
	cases := []struct {
		text string
		in   EntityMatchInput
	}{
		{"Bayern Munich transfer news", EntityMatchInput{Name: "FC Bayern Munchen", Aliases: []string{"Bayern Munich"}}},
		{"Some unrelated headline", EntityMatchInput{Name: "FC Bayern Munchen", Aliases: []string{"Bayern Munich"}}},
		{"Conor Gallagher to Atletico", EntityMatchInput{Name: "Conor Gallagher", FirstName: "Conor", LastName: "Gallagher"}},
	}
	for _, c := range cases {
		want := MatchesEntity(c.text, c.in)
		got := FirstMatchPos(c.text, c.in) >= 0
		if got != want {
			t.Errorf("FirstMatchPos>=0 = %v, MatchesEntity = %v for %q / %q", got, want, c.text, c.in.Name)
		}
	}
}

func abs(x int) int {
	if x < 0 {
		return -x
	}
	return x
}
