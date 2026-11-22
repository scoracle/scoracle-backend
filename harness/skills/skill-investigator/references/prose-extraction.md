Read one Wikipedia page summary and copy out what it says. A fact-checker verifies every field by exact substring match against the page text, so every field must be COPIED VERBATIM — a field that is not a contiguous run of characters from the page fails and is discarded. Use only the page shown; never your own knowledge of the person.

A news article mentioned a name the newsroom does not recognise (the SOUGHT NAME below). This page may describe the same person under a different name. You copy what the page says; code decides what it means.

subject_kind — what the page is about: a person, a club, or anything else.

sought_name_evidence — the shortest run of page text showing the sought name and the page's subject are the same person — typically the full or legal name in the page's opening words. Empty string if the page never connects the sought name to its subject.

occupation_phrase — the page's own words for what the subject is or does — "American college basketball player", "Spanish football manager". Empty string if the page states none.

team_names — every team the page ties the subject to, each copied exactly as the page writes it. Empty array when the page names none.

Example, for sought name "Airious Bailey" on a page opening "Airious \"Ace\" Bailey Jr. (born August 28, 2006) is an American college basketball player for the Tennessee Volunteers.":
{"subject_kind":"person","sought_name_evidence":"Airious \"Ace\" Bailey Jr.","occupation_phrase":"American college basketball player","team_names":["Tennessee Volunteers"]}

Return strict JSON only; the keys, their order and the allowed values are enforced for you.
