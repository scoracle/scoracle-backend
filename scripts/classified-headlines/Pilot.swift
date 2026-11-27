import Foundation
import FoundationModels

// Build alongside the iOS app's EntityChatPayload.swift (see README.md).
@main
struct Pilot {
    typealias Object = [String: Any]

    static func json(_ value: Any) throws -> String {
        let data = try JSONSerialization.data(withJSONObject: value, options: [.sortedKeys, .withoutEscapingSlashes])
        return String(decoding: data, as: UTF8.self)
    }

    static func load(_ path: String) throws -> Object {
        let data = try Data(contentsOf: URL(fileURLWithPath: path))
        return try JSONSerialization.jsonObject(with: data) as! Object
    }

    struct FrozenAPI: ChatAPI {
        let data: String
        func entities() async throws -> [DirectoryEntry] { [] }
        func payload(for entity: Entity, skill: ReadingSkill) async throws -> String { data }
    }

    @MainActor
    static func run(_ name: String, packet: String, questions: [String], skill: ReadingSkill = .news) async throws {
        let model = SystemLanguageModel.default
        let tokens = try await model.tokenCount(for: packet)
        if !CommandLine.arguments.contains("--session") {
            let entity = skill == .transfers ? Entity(sport: .football, kind: .player, key: "990003", name: "Alex Sample")
                : name.hasPrefix("chelsea/") ? Entity(sport: .football, kind: .team, key: "49", name: "Chelsea")
                : Entity(sport: .football, kind: .team, key: "990001", name: "Arsenal")
            var prior: ConversationExchange?
            let workspace = Workspace()
            for question in questions {
                let request = ReadingSkill.Request(question: question, entity: entity, prior: prior,
                    api: FrozenAPI(data: packet), engine: AppleFoundationLanguageModel())
                var row: Object = ["case": name, "packet_tokens": tokens, "question": question]
                do {
                    let result = try await workspace.run(skill, request: request)
                    row["answer"] = result.product.text
                    row["attempts"] = result.receipt.attempts
                    prior = ConversationExchange(scope: entity.id, question: question, answer: result.product.text, evidence: packet)
                } catch {
                    row["error"] = String(describing: error)
                    prior = nil
                }
                print(try json(row))
            }
            return
        }
        let session = LanguageModelSession(model: model, instructions: """
        Answer the user's sports questions using only the supplied evidence. Attribute claims.
        Distinguish what was reported from what was confirmed. If the evidence doesn't say, say so.
        Classifier scores describe headline wording and relevance, not probabilities of events.
        """)
        for (turn, question) in questions.enumerated() {
            let prompt = turn == 0 ? "Evidence:\n\(packet)\nQuestion: \(question)" : question
            let response = try await session.respond(to: prompt, options: GenerationOptions(temperature: 0, maximumResponseTokens: 384))
            print(try json(["case": name, "packet_tokens": tokens, "question": question, "answer": response.content]))
        }
    }

    static func main() async throws {
        guard case .available = SystemLanguageModel.default.availability else {
            throw NSError(domain: "Apple Intelligence unavailable", code: 1)
        }
        let dir = "harness/docs/qualification/2026-11-15-headline-pipeline/"
        let news = try load(dir + "journalist-api.json")
        let intake = try String(contentsOfFile: dir + "sample-intake_test.go", encoding: .utf8)
        let rows = intake.matches(of: /rssFixtureItem\{"([^"]+)","([^"]+)"/).map {
            ["headline": String($0.1), "publisher": String($0.2), "published_at": "2026-11-15T15:28:00-05:00"]
        }
        precondition(rows.count == 3)
        let target = ["name": "Arsenal", "id": "football/team/990001"]
        let questions = ["What changed with Alex Sample?", "Which outlet reported that, and what exactly was its headline?", "Has he signed or passed the medical?"]
        try await run("arsenal/published", packet: EntityChatPayload.compact(["target": target, "narratives": news]), questions: questions)
        try await run("arsenal/sources", packet: json(["target": target, "headlines": rows]), questions: questions)
        if !CommandLine.arguments.contains("--session") {
            try await run("arsenal/focused", packet: json(["target": target, "headlines": [rows[2]]]), questions: questions)
            let vibe = try load(dir + "influencer-api.json")
            let current = vibe["current"] as! Object
            let moodQuestions = ["How did the coverage feel?", "What headline supports that? Does it tell us every fan agrees?"]
            try await run("arsenal/vibe-published", packet: EntityChatPayload.compact(["target": target, "vibes": vibe]), questions: moodQuestions, skill: .mood)
            try await run("arsenal/vibe-sources", packet: json(["target": target, "vibe": current.filter {
                ["score", "score_scale", "reporting_start", "reporting_end"].contains($0.key)
            }, "headlines": [rows[1]]]), questions: moodQuestions, skill: .mood)
            let wire = try load(dir + "insider-api.json")
            let transfer = (wire["transfers"] as! [Object])[0]
            let player = ["name": "Alex Sample", "id": "football/player/990003"]
            let transferQuestions = ["Which club is trying to sign Alex Sample?", "Did he pass a medical or sign a contract?"]
            try await run("arsenal/transfer-published", packet: EntityChatPayload.compact(["target": player, "transfers": wire]), questions: transferQuestions, skill: .transfers)
            try await run("arsenal/transfer-sources", packet: json(["target": player, "stage": transfer["stage"]!, "status": (transfer["heat_components"] as! Object)["status"]!, "headlines": [rows[2]]]), questions: transferQuestions, skill: .transfers)
            // Compare the very same packet and questions, changing only the scope statement.
            try await run("arsenal/transfer-scoped", packet: json(["target": player, "stage": transfer["stage"]!, "status": (transfer["heat_components"] as! Object)["status"]!, "headlines": [rows[2]], "evidence_scope": "Publisher headlines only. Reported stage is Scoracle's reading of coverage. Unmentioned outcomes are unknown. A planned event is not a completed event."]), questions: transferQuestions, skill: .transfers)
        }

        let capture = try load("harness/fixtures/character-runs/chelsea-2026-10-20/journalist-input.json")
        let prompt = try JSONSerialization.jsonObject(with: Data((capture["prompt"] as! String).utf8)) as! Object
        let reports = prompt["fresh"] as! [Object]
        precondition(reports.count == 24)
        let followups = ["What is reported about Gessime Yassine?", "Who reported it? Is there evidence that the unnamed forward who signed was Yassine?", "What percentage chance does he have of signing?", "Did Bediako leave Stamford Bridge for Atletico Madrid?"]
        for variant in ["earlier", "latest", "latest-scored"] {
            let selected = reports.filter {
                variant != "earlier" || ($0["published_at"] as! String) <= "2026-10-20T13:10:00Z"
            }
            let projected = selected.map { report in
                var row = report.filter { ["headline", "publisher", "published_at"].contains($0.key) }
                if variant == "latest-scored", let world = report["classifier_world"] as? Object,
                   let spectrum = world["spectrum"] as? [String: Double] {
                    row["movement_score"] = spectrum["movement"]?.rounded()
                    row["subject_relevance_score"] = spectrum["subject"]?.rounded()
                }
                return row
            }
            try await run("chelsea/" + variant, packet: json(["target": "Chelsea FC", "headlines": projected]), questions: followups)
            if variant == "latest", !CommandLine.arguments.contains("--session") {
                try await run("chelsea/latest-scoped", packet: json(["target": "Chelsea FC", "headlines": projected,
                    "evidence_scope": "Publisher headlines only. Items are separate claims. Do not identify an unnamed person from another headline. Similar club names do not establish identity. Missing odds or outcomes are unknown."]), questions: followups)
            }
        }
    }
}
