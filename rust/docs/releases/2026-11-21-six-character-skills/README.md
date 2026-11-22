# Six-character production release

Activated November 21, 2026 around 11:28 p.m. Eastern. The Mac workers share their existing slots across Journalist and Scout, Influencer and Analyst, and Insider and Oracle. Archbox also runs Insider. The API runs release `character-skills-20261121-v1`. At 11:47 p.m. Eastern the Mac workers, Archbox writer, and Archbox event dispatcher switched to `character-skills-20261121-v3`.

Run [health.sql](health.sql) to see published and scored rows since activation, the latest timestamp and a target to inspect for each character. The query uses the same qualified views as the API. A news card with multiple paragraphs can occupy several rows. Activity scores identify the new Journalist and Insider publications even though the prompt versions stayed the same.

Versions are `n149-full-profile`, `vibe-frame-v65-full-profile`, `insider-source-v47-full-profile`, `s98-stat-direction`, `momentum-s56-card-scores` and `or44-finished-cards`. Database migrations 334 (card scores), 335 (Analyst momentum), and 336 (Oracle finished-card dependencies) were applied in production. The API source was built on `5305f68a`, preserving its deployed entity read optimizations. The worker source was ported onto the deployed plugin layout. The Mac’s previous launchd plists are in the new release’s `rollback` directory. The previous Archbox service overrides and binaries are retained.

In continuous publishing, Oracle reads the latest finished cards for the target and refreshes when a new peer card lands. Scheduled editions retain their five stage dependencies. Character workers decode the structured model reply and save its prose directly. Source and transfer links must resolve to real records.

At 11:50 p.m. Eastern all six production API routes served newly generated prose. Checked Journalist on football team 10, Influencer on NFL player 1581, Insider on NFL player 685, Scout and Analyst on NBA team 30, and Oracle on football team 1079. Six Insider reports across four targets were requeued from the former model-reply hold. Analyst snapshots without a pair of dated scores have no measured momentum.

## Analyst replay

These are verbatim 4B generations in an isolated replay using recorded Influencer scores. The three real teams had only one scored Scout reading, so tone supplied their only measurable change. Harbor is fictional. The team calls used temperature zero. Harbor used the production temperature of 0.3.

**Chargers · Tone 50 → 15 · momentum 32.5/100**

> The coverage tone on the Los Angeles Chargers plummeted by 35 points, shifting from a lukewarm 50 to a sharply negative 15, signaling that the media has turned from polite observers to outright critics. In plain sports terms, the narrative has flipped from 'okay' to 'trouble,' and the press box is now full of people wondering if the team's season is over before it even started.

**Bucks · Tone 25 → 42 · momentum 58.5/100**

> The Milwaukee Bucks' coverage tone jumped 17 points higher overnight, signaling a sharp shift in the narrative from neutral to noticeably positive. With a current tone score of 42, the story is still balanced but clearly leaning toward the team's favor as the week progresses.

**Heat · Tone 75 → 45 · momentum 35/100**

> Miami Heat coverage tone plummeted 30 points from yesterday to today, signaling a sharp shift in the narrative against the backdrop of a current sentiment score of 45 that reads as lukewarm at best. The story isn't just cooling down; it's actively freezing, leaving fans wondering if the heat has finally turned into a cold draft.

**Harbor · Performance +8, tone −12 · momentum 49/100**

> Harbor's performance score jumped 8 points to 68, signaling a real uptick in on-court grit, while the coverage tone tumbled 12 points to 38, suggesting the hype machine is running hot while the scoreboard is still catching up.

The Bucks gained 17 points of momentum on coverage tone. In the Harbor control the model got both numeric directions right, then reversed their implication in its closing metaphor.
