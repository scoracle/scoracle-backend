package thirdparty
import("context";"os";"testing";"time";"net/http";"github.com/jackc/pgx/v5/pgxpool")
func TestLiveSampleIntake(t *testing.T){
 ctx:=context.Background(); pool,err:=pgxpool.New(ctx,os.Getenv("TEST_DATABASE_URL")); if err!=nil {t.Fatal(err)}; defer pool.Close()
 var name string; if err=pool.QueryRow(ctx,"SELECT current_database()").Scan(&name);err!=nil||name!="classifier_test_coverage_samples2"{t.Fatal("requires isolated sample database",err)}
 _,err=pool.Exec(ctx,`INSERT INTO sports(id,display_name,current_season) VALUES('FOOTBALL','Football',2026) ON CONFLICT DO NOTHING;
 INSERT INTO teams(id,sport,name) VALUES(990001,'FOOTBALL','Arsenal'),(990002,'FOOTBALL','Chelsea') ON CONFLICT DO NOTHING;
 INSERT INTO players(id,sport,name,first_name,last_name) VALUES(990003,'FOOTBALL','Alex Sample','Alex','Sample') ON CONFLICT DO NOTHING;
 UPDATE players SET team_id=990002 WHERE id=990003 AND sport='FOOTBALL';
 INSERT INTO entity_name_surfaces(sport,entity_type,entity_id,norm,surface_kind) VALUES('FOOTBALL','team',990001,'arsenal','name'),('FOOTBALL','team',990002,'chelsea','name'),('FOOTBALL','player',990003,'alex sample','name');
 UPDATE coverage_schedule SET enabled=true,enrolled_at=clock_timestamp(),phase='coverage',edition_date=NULL,collection_cutoff='23:59',required_stages=ARRAY['narratives','vibe','transfers'];`);if err!=nil {t.Fatal(err)}
 now:=time.Now().Add(-time.Minute)
 body:=rssFixture(
 rssFixtureItem{"Arsenal confirm captain will miss two weeks with ankle injury","Sample Sports Desk","https://example.invalid/sample/injury",now,"Arsenal confirmed their captain injured his ankle in training and will be unavailable for two weeks."},
 rssFixtureItem{"Arsenal supporters celebrate late winner and praise team's fighting spirit","Sample Fan Report","https://example.invalid/sample/fans",now,"Arsenal supporters celebrated a late winning goal and expressed renewed optimism about their team's season."},
 rssFixtureItem{"Arsenal agree fee with Chelsea to sign Alex Sample, medical scheduled tomorrow","Sample Transfer Desk","https://example.invalid/sample/transfer",now,"Arsenal and Chelsea have agreed a transfer fee for Alex Sample. The player will undergo a medical tomorrow before signing his contract."})
 s:=newFixtureService(t,func(w http.ResponseWriter,r *http.Request){w.Header().Set("Content-Type","application/rss+xml");w.Write([]byte(body))});s.pool=pool
 ids,f,err:=s.GetEntityNews(ctx,"team",990001,"Arsenal","FOOTBALL",0,nil)
 if err!=nil||len(ids)!=3 {t.Fatalf("ids=%v funnel=%+v err=%v",ids,f,err)}
 t.Logf("sample article IDs=%v funnel=%+v",ids,f)
}
