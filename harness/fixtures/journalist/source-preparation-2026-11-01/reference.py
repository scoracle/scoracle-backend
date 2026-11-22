"""Manually reviewed source-only diagnostic preparation; not a scalable production step."""
def s(actor,kind,claim,*qualifications):return {'actor':actor,'kind':kind,'claim':claim,'qualifications':list(qualifications)}
def p(statements,*unspecified):return {'statements':statements,'unspecified':list(unspecified)}
REFERENCE={
1276620:p([s('AC Milan, Arsenal','assertion','The headline reports a move for Tresoldi involving AC Milan and Arsenal, mentioning an unexplained 60 million figure and timing as early as January.','as early as January')],'meaning and unit of 60 million'),
1424030:p([s('AC Milan','assertion',"AC Milan collapsed 0-3 in the Europa League after Maignan's red card.")]),
1426503:p([s('Bournemouth','assertion','Bournemouth beat 10-man Milan in the Europa League after a goalkeeper was sent off.'),s('Juventus','assertion','Juventus were held to a draw.')],'identity of the goalkeeper','Milan match score'),
1414585:p([s('Bournemouth','assertion','Bournemouth beat 10-man Milan in the Europa League after a goalkeeper was sent off.'),s('Juventus','assertion','Juventus were held to a draw.')],'identity of the goalkeeper','Milan match score'),
1415284:p([s('Bournemouth','assertion','Bournemouth beat AC Milan 3-0, with Maignan seeing red as the Rossoneri imploded on the south coast.')],'competition','Maignan role'),
1394917:p([s('Milan','assertion',"Milan forgot to make a substitution after Maignan saw red; the headline includes the reaction 'Absolutely farcical!'.")],'speaker of the quoted reaction'),
1374081:p([s('Bournemouth','assertion','Bournemouth took an early lead against Milan in a Europa League clash.','early lead')],'final result'),
1363845:p([s('Bournemouth','assertion','Bournemouth hammered 10-man AC Milan in the Europa League.'),s('Juventus','assertion','Juventus were denied at the death.')],'match scores'),
1425437:p([s('Pavlovic','opinion','Pavlovic insists Milan are on the right track despite too many mistakes being made.','even if','‘too many mistakes’')],'Pavlovic first name and role'),
1341401:p([s('Donte Jackson','opinion',"A possible Donte Jackson trade might turn the Bears' defense into a Super Bowl threat.",'might')]),
1352005:p([s(None,'listing','A Thursday Night Football Live Show is listed for Patriots at Bears.')],'whether the listing is a game broadcast'),
1432888:p([s('Bears','assertion','The Bears announced a move to honor Mike Ditka during the Thursday Night game.','Announce Move to Honor')],'details of the tribute'),
1412576:p([s('Bears','question','The headline asks how the Bears are honoring Mike Ditka on Thursday night.')],'details of how they are honoring Ditka'),
1262232:p([s('Chicago Bears','question','The headline concerns how the Chicago Bears are honoring Mike Ditka on TNF.')],'details of how they are honoring Ditka'),
1289440:p([s('Max Strus','assertion',"Max Strus is ruled out for the Clippers' road trip.")],'reason for absence'),
1429100:p([s(None,'listing','An Orlando Magic versus LA Clippers game page is listed for Jan 15, 2027.')],'game completion and result'),
1398531:p([s(None,'listing','A Toronto Raptors versus LA Clippers game-summary page is listed for Nov 2, 2026.')],'game completion and result'),
1325124:p([s('AC Milan','assertion',"AC Milan back a fan boycott over away ticket prices described as excessive.")]),
1208973:p([s('AC Milan','assertion',"AC Milan back a fan boycott over away ticket prices described as excessive, and warn of retaliation.")]),
1189629:p([s('Milan','assertion','Milan fear a lengthy lay-off for Chukwueze.','fear'),s('Athekame','question','The headline asks whether Athekame could return in January; two factors are needed.','could','Two factors are needed')],'what the two factors are'),
1189646:p([s('AC Milan','question','The headline asks about AC Milan recalling Athekame in January, while an early OL exit is deemed unlikely.','Early OL exit deemed unlikely')]),
1115631:p([s('AC Milan','assertion',"AC Milan are eager to secure Rabiot's future with a fresh contract.",'eager')],'whether negotiations or agreement exist'),
1410388:p([s('Christian Gonzalez','assertion','Christian Gonzalez is good to go despite a newly-announced illness.','despite newly-announced illness')]),
1410389:p([s('Mike Vrabel','assertion',"Patriots' Mike Vrabel is set to snap a 27-year streak on TNF versus the Bears.",'set to')],'type of streak','Mike Vrabel role'),
1330977:p([s('Chicago Bears','question',"The headline concerns the Chicago Bears' possible move from Soldier Field.",'possible move')]),
1260557:p([s('Darius Acuff Jr.','assertion','Darius Acuff Jr. had a debut against the Clippers described as brutal.')]),
1338223:p([s('Keegan Murray','assertion','Keegan Murray struggled mightily against the Clippers.')]),
1221840:p([s('Clippers fans','question','The headline concerns how Clippers fans can get free parking and new jerseys on opening night.')],'how fans obtain them'),
1192912:p([s('L.A. Clippers','assertion','The L.A. Clippers usher in a new season with a jersey swap and free parking.')])
}
