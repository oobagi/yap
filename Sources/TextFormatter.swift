import Foundation

enum FormattingStyle: String, CaseIterable {
    case verbatim = "verbatim"
    case casual = "casual"
    case professional = "professional"
    
    var label: String {
        switch self {
        case .verbatim: return "Verbatim (no formatting)"
        case .casual: return "Casual"
        case .professional: return "Professional"
        }
    }
    
    var prompt: String {
        switch self {
        case .verbatim:
            return "" // unused
        case .casual:
            return """
            You clean up spoken text. You MUST respond with ONLY a JSON object: {"text":"cleaned version here"} \
            Rules for cleaning: remove filler sounds (um, uh, er). Keep casual phrases (like, you know). \
            All lowercase. Minimal punctuation. Keep contractions and slang. Keep the speaker's exact words. \
            PRESERVE all existing punctuation and symbols — parentheses, quotes, brackets, etc. Do not remove them. \
            NEVER respond conversationally. NEVER add commentary. ONLY output the JSON object.
            """
        case .professional:
            return """
            You clean up spoken text. You MUST respond with ONLY a JSON object: {"text":"cleaned version here"} \
            Rules for cleaning: remove filler words (um, uh, like, you know). Fix grammar. \
            Proper punctuation and capitalization. Expand contractions. Keep the speaker's original meaning. \
            PRESERVE all existing punctuation and symbols — parentheses, quotes, brackets, etc. Do not remove them. \
            NEVER respond conversationally. NEVER add commentary. ONLY output the JSON object.
            """
        }
    }
}

enum APIProvider: String, CaseIterable {
    case none = "none"
    case openai = "openai"
    case anthropic = "anthropic"
    
    var label: String {
        switch self {
        case .none: return "None (no formatting)"
        case .openai: return "OpenAI"
        case .anthropic: return "Anthropic"
        }
    }
    
    var defaultModel: String {
        switch self {
        case .none: return ""
        case .openai: return "gpt-4o-mini"
        case .anthropic: return "claude-haiku-4-5-20251001"
        }
    }
    
    var defaultEndpoint: String {
        switch self {
        case .none: return ""
        case .openai: return "https://api.openai.com/v1/chat/completions"
        case .anthropic: return "https://api.anthropic.com/v1/messages"
        }
    }
}

class TextFormatter {
    private let provider: APIProvider
    private let apiKey: String
    private let model: String
    private let endpoint: String
    private let style: FormattingStyle
    
    init(provider: APIProvider, apiKey: String, model: String? = nil, endpoint: String? = nil, style: FormattingStyle) {
        self.provider = provider
        self.apiKey = apiKey
        self.model = model ?? provider.defaultModel
        self.endpoint = endpoint ?? provider.defaultEndpoint
        self.style = style
    }
    
    /// Format transcribed text using the configured AI provider.
    /// If style is verbatim or provider is none, returns the original text.
    func format(_ text: String, completion: @escaping (Result<String, Error>) -> Void) {
        log("format() called — provider=\(provider.rawValue) style=\(style.rawValue) apiKey=\(apiKey.isEmpty ? "EMPTY" : "set (\(apiKey.prefix(10))...)")")
        guard provider != .none, style != .verbatim, !apiKey.isEmpty else {
            log("format() guard failed — skipping API call")
            completion(.success(text))
            return
        }
        
        // Don't send very short/empty text to the API — model hallucinates
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard trimmed.count >= 3 else {
            log("format() text too short (\(trimmed.count) chars) — skipping API call")
            completion(.success(text))
            return
        }
        
        log("format() making \(provider.rawValue) API call...")
        switch provider {
        case .openai:
            callOpenAI(text: text, completion: completion)
        case .anthropic:
            callAnthropic(text: text, completion: completion)
        case .none:
            completion(.success(text))
        }
    }
    
    // MARK: - OpenAI
    
    private func callOpenAI(text: String, completion: @escaping (Result<String, Error>) -> Void) {
        guard let url = URL(string: endpoint) else {
            completion(.failure(FormatterError.invalidEndpoint))
            return
        }
        
        var request = URLRequest(url: url)
        request.httpMethod = "POST"
        request.timeoutInterval = 10
        request.setValue("Bearer \(apiKey)", forHTTPHeaderField: "Authorization")
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        
        let body: [String: Any] = [
            "model": model,
            "messages": [
                ["role": "system", "content": style.prompt],
                ["role": "user", "content": "<input>\(text)</input>"]
            ],
            "max_tokens": 2048,
            "temperature": 0.3
        ]
        
        request.httpBody = try? JSONSerialization.data(withJSONObject: body)
        
        URLSession.shared.dataTask(with: request) { data, response, error in
            if let error = error {
                completion(.failure(error))
                return
            }
            guard let data = data,
                  let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
                  let choices = json["choices"] as? [[String: Any]],
                  let message = choices.first?["message"] as? [String: Any],
                  let content = message["content"] as? String else {
                // If parsing fails, return original text rather than failing
                print("[VoiceType] OpenAI response parse failed, using raw transcription")
                completion(.success(text))
                return
            }
            completion(.success(content.trimmingCharacters(in: .whitespacesAndNewlines)))
        }.resume()
    }
    
    // MARK: - Anthropic
    
    private func callAnthropic(text: String, completion: @escaping (Result<String, Error>) -> Void) {
        guard let url = URL(string: endpoint) else {
            completion(.failure(FormatterError.invalidEndpoint))
            return
        }
        
        var request = URLRequest(url: url)
        request.httpMethod = "POST"
        request.timeoutInterval = 10
        request.setValue(apiKey, forHTTPHeaderField: "x-api-key")
        request.setValue("2023-06-01", forHTTPHeaderField: "anthropic-version")
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        
        let body: [String: Any] = [
            "model": model,
            "system": style.prompt,
            "messages": [
                ["role": "user", "content": "<input>\(text)</input>"],
                ["role": "assistant", "content": "{"]
            ],
            "max_tokens": 2048,
            "temperature": 0.0,
            "stop_sequences": ["}"]
        ]
        
        request.httpBody = try? JSONSerialization.data(withJSONObject: body)
        
        URLSession.shared.dataTask(with: request) { data, response, error in
            if let error = error {
                log("Anthropic API error: \(error)")
                completion(.failure(error))
                return
            }
            if let httpResponse = response as? HTTPURLResponse {
                log("Anthropic API status: \(httpResponse.statusCode)")
            }
            guard let data = data else {
                log("Anthropic API: no data")
                completion(.success(text))
                return
            }
            let rawResponse = String(data: data, encoding: .utf8) ?? "unreadable"
            log("Anthropic API response: \(rawResponse.prefix(200))")
            guard let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
                  let content = json["content"] as? [[String: Any]],
                  let textBlock = content.first?["text"] as? String else {
                log("Anthropic response parse failed, using raw transcription")
                completion(.success(text))
                return
            }
            // Response is prefilled with "{" and stopped at "}", so textBlock is inner JSON
            let fullJSON = "{\(textBlock)}"
            log("Anthropic parsed JSON: \(fullJSON.prefix(300))")
            if let innerData = fullJSON.data(using: .utf8),
               let innerJSON = try? JSONSerialization.jsonObject(with: innerData) as? [String: String],
               let cleaned = innerJSON["text"], !cleaned.isEmpty {
                completion(.success(cleaned))
            } else {
                // Fallback: use raw text block
                let fallback = textBlock.trimmingCharacters(in: .whitespacesAndNewlines)
                log("JSON parse failed, falling back to raw: \(fallback.prefix(200))")
                completion(.success(fallback))
            }
        }.resume()
    }
}

enum FormatterError: LocalizedError {
    case invalidEndpoint
    
    var errorDescription: String? {
        switch self {
        case .invalidEndpoint:
            return "Invalid API endpoint URL"
        }
    }
}
