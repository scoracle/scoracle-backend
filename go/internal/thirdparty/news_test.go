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
		{"FC Bayern Munchen", []string{"FCB"}, ""},          // too short
		{"Bayern Munich", []string{"bayern munich"}, ""},     // same as primary
		{"Arsenal", nil, ""},                                  // no aliases
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
