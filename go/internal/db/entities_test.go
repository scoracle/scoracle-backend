package db

import (
	"os"
	"strings"
	"testing"
)

func TestUniversalEntitiesStatementContract(t *testing.T) {
	stmt := universalEntitiesStatement

	required := []string{
		"'page', 'entities'",
		"'total_entities'",
		"'entities'",
		"'id', id",
		"'type', type",
		"'sport', sport",
		"'name', name",
		"'aliases', to_jsonb(aliases)",
		"'search_tokens', to_jsonb(search_tokens)",
		"FROM public.players p",
		"FROM public.teams t",
		"LEFT JOIN public.leagues l",
		"p.sport IN ('NBA', 'NFL', 'FOOTBALL')",
		"t.sport IN ('NBA', 'NFL', 'FOOTBALL')",
		"lower(p.sport) AS sport",
		"lower(t.sport) AS sport",
		"UNION ALL",
	}
	for _, needle := range required {
		if !strings.Contains(stmt, needle) {
			t.Fatalf("universal entities statement missing %q", needle)
		}
	}
}

func TestUniversalEntitiesStatementIsLightweight(t *testing.T) {
	forbiddenJSONKeys := []string{
		"'meta'",
		"'stat_definitions'",
		"'photo_url'",
		"'logo_url'",
		"'leagues'",
		"'venue_name'",
		"'venue_capacity'",
		"'raw_response'",
	}
	for _, needle := range forbiddenJSONKeys {
		if strings.Contains(universalEntitiesStatement, needle) {
			t.Fatalf("universal entities statement must not emit heavy field %q", needle)
		}
	}
}

func TestUniversalEntitiesStatementDoesNotReuseSportAutofillViews(t *testing.T) {
	for _, needle := range []string{
		"nba.autofill_entities",
		"nfl.autofill_entities",
		"football.autofill_entities",
	} {
		if strings.Contains(universalEntitiesStatement, needle) {
			t.Fatalf("universal entities statement must not reuse stat-filtered sport autofill view %q", needle)
		}
	}
}

func TestCurrentIdentityContractInDBStatements(t *testing.T) {
	srcBytes, err := os.ReadFile("db.go")
	if err != nil {
		t.Fatalf("read db.go: %v", err)
	}
	src := string(srcBytes)

	if !strings.Contains(universalEntitiesStatement, "public.player_current_identity") {
		t.Fatalf("universal entities must resolve team/position from player_current_identity")
	}
	if strings.Contains(src, "SELECT ps.team_id FROM public.player_stats ps") {
		t.Fatalf("db statements must not derive current team directly from latest player_stats")
	}
	for _, needle := range []string{
		"LEFT JOIN public.player_current_identity pci",
		"pci.team_id = l.team_id AND COALESCE(l.direction, '') = 'incoming'",
		"pci.team_id <> l.team_id AND COALESCE(l.direction, '') = 'outgoing'",
	} {
		if !strings.Contains(src, needle) {
			t.Fatalf("db transfer reads missing current-identity guard %q", needle)
		}
	}
}
