import Foundation

enum FormattingStyle: String, CaseIterable {
    case verbatim = "verbatim"
    case casual = "casual"
    case formatted = "formatted"
    case professional = "professional"
    
    var label: String {
        switch self {
        case .verbatim: return "Verbatim"
        case .casual: return "Casual"
        case .formatted: return "Formatted"
        case .professional: return "Professional"
        }
    }
    
    var description: String {
        switch self {
        case .verbatim: return "Raw dictation, no changes at all"
        case .casual: return "Light cleanup, keeps your voice"
        case .formatted: return "Clean formatting, faithful to what you said"
        case .professional: return "Polished writing, elevated language"
        }
    }
    
    /// Example showing what each mode produces from the same input
    static let exampleInput = "um so like i was thinking we should probably you know move the meeting to friday because uh thursdays not gonna work for me"
    
    var exampleOutput: String {
        switch self {
        case .verbatim:
            return "um so like i was thinking we should probably you know move the meeting to friday because uh thursdays not gonna work for me"
        case .casual:
            return "so like i was thinking we should probably move the meeting to friday because thursdays not gonna work for me"
        case .formatted:
            return "So I was thinking we should probably move the meeting to Friday, because Thursday's not going to work for me."
        case .professional:
            return "I believe we should reschedule the meeting to Friday, as Thursday will not work for my schedule."
        }
    }
    
    var prompt: String {
        switch self {
        case .verbatim:
            return "" // unused
        case .casual:
            return """
            You clean up spoken text. You MUST respond with ONLY a JSON object: {"text":"cleaned version here"} \
            Rules: remove ONLY filler sounds (um, uh, er). Keep everything else exactly as spoken — \
            casual phrases, slang, sentence structure, contractions. All lowercase. Minimal punctuation. \
            PRESERVE all existing symbols — parentheses, quotes, brackets, etc. \
            NEVER respond conversationally. ONLY output the JSON object.
            """
        case .formatted:
            return """
            You clean up spoken text. You MUST respond with ONLY a JSON object: {"text":"cleaned version here"} \
            Rules: remove filler words (um, uh, er, like, you know). Fix punctuation and capitalization. \
            Keep the speaker's EXACT words and sentence structure — do not rephrase or rewrite. \
            Keep contractions as spoken. Only fix obvious grammar errors. \
            PRESERVE all existing symbols — parentheses, quotes, brackets, etc. \
            NEVER respond conversationally. ONLY output the JSON object.
            """
        case .professional:
            return """
            You clean up spoken text. You MUST respond with ONLY a JSON object: {"text":"cleaned version here"} \
            Rules: remove all filler words. Elevate the language to sound polished and professional. \
            Fix grammar, improve word choice, use proper punctuation and capitalization. \
            Expand contractions. You MAY rephrase for clarity and professionalism, but keep the original meaning. \
            PRESERVE all existing symbols — parentheses, quotes, brackets, etc. \
            NEVER respond conversationally. ONLY output the JSON object.
            """
        }
    }
    
    /// Prompt for Gemini audio transcription (transcribe + format in one step)
    var geminiPrompt: String {
        let noiseRule = "IGNORE all background noise, sound effects, music, and non-speech sounds. " +
            "Only transcribe human speech. If there is no speech, respond with {\"text\":\"\"}."
        
        switch self {
        case .verbatim:
            return """
            Transcribe this audio exactly as spoken. Include all filler words (um, uh, like, you know). \
            All lowercase. No punctuation unless clearly intended. \
            \(noiseRule) \
            You MUST respond with ONLY a JSON object: {"text":"transcription here"}
            """
        case .casual:
            return """
            Transcribe this audio. Remove filler sounds (um, uh, er) but keep everything else exactly as spoken — \
            casual phrases, slang, sentence structure, contractions. All lowercase. Minimal punctuation. \
            PRESERVE any symbols the speaker mentions — parentheses, quotes, brackets, etc. \
            \(noiseRule) \
            You MUST respond with ONLY a JSON object: {"text":"transcription here"}
            """
        case .formatted:
            return """
            Transcribe this audio. Remove filler words (um, uh, er, like, you know). \
            Fix punctuation and capitalization. Keep the speaker's EXACT words and sentence structure — \
            do not rephrase or rewrite. Keep contractions as spoken. Only fix obvious grammar errors. \
            PRESERVE any symbols the speaker mentions — parentheses, quotes, brackets, etc. \
            \(noiseRule) \
            You MUST respond with ONLY a JSON object: {"text":"transcription here"}
            """
        case .professional:
            return """
            Transcribe this audio. Remove all filler words. Elevate the language to sound polished and professional. \
            Fix grammar, improve word choice, use proper punctuation and capitalization. \
            Expand contractions. You MAY rephrase for clarity and professionalism, but keep the original meaning. \
            PRESERVE any symbols the speaker mentions — parentheses, quotes, brackets, etc. \
            \(noiseRule) \
            You MUST respond with ONLY a JSON object: {"text":"transcription here"}
            """
        }
    }
}

enum APIProvider: String, CaseIterable {
    case gemini = "gemini"
    case openai = "openai"
    case anthropic = "anthropic"
    
    var label: String {
        switch self {
        case .gemini: return "Google Gemini"
        case .openai: return "OpenAI"
        case .anthropic: return "Anthropic"
        }
    }
    
    var defaultModel: String {
        switch self {
        case .gemini: return "gemini-2.5-flash"
        case .openai: return "gpt-4o-mini"
        case .anthropic: return "claude-haiku-4-5-20251001"
        }
    }
    
    var defaultEndpoint: String {
        switch self {
        case .gemini: return "https://generativelanguage.googleapis.com/v1beta"
        case .openai: return "https://api.openai.com/v1/chat/completions"
        case .anthropic: return "https://api.anthropic.com/v1/messages"
        }
    }
    
    /// Whether this provider handles transcription itself (skips Apple Speech)
    var handlesTranscription: Bool {
        switch self {
        case .gemini: return true
        default: return false
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
    
    /// Whether this formatter handles transcription from audio directly
    var handlesTranscription: Bool {
        return provider.handlesTranscription && !apiKey.isEmpty
    }
    
    /// Transcribe + format audio in one step (Gemini only)
    func transcribeAndFormat(audioURL: URL, completion: @escaping (Result<String, Error>) -> Void) {
        log("transcribeAndFormat() — provider=\(provider.rawValue) style=\(style.rawValue)")
        guard provider == .gemini, !apiKey.isEmpty else {
            completion(.failure(FormatterError.unsupportedProvider))
            return
        }
        callGeminiAudio(audioURL: audioURL, completion: completion)
    }
    
    /// Format already-transcribed text (OpenAI / Anthropic)
    func format(_ text: String, completion: @escaping (Result<String, Error>) -> Void) {
        log("format() called — provider=\(provider.rawValue) style=\(style.rawValue)")
        guard provider != .gemini, style != .verbatim, !apiKey.isEmpty else {
            completion(.success(text))
            return
        }
        
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard trimmed.count >= 3 else {
            log("format() text too short (\(trimmed.count) chars) — skipping")
            completion(.success(text))
            return
        }
        
        switch provider {
        case .openai:
            callOpenAI(text: text, completion: completion)
        case .anthropic:
            callAnthropic(text: text, completion: completion)
        default:
            completion(.success(text))
        }
    }
    
    // MARK: - Gemini (audio → text, one-shot)
    
    private func callGeminiAudio(audioURL: URL, completion: @escaping (Result<String, Error>) -> Void) {
        guard let audioData = try? Data(contentsOf: audioURL) else {
            completion(.failure(FormatterError.audioReadFailed))
            return
        }
        
        let base64Audio = audioData.base64EncodedString()
        let urlString = "\(endpoint)/models/\(model):generateContent?key=\(apiKey)"
        
        guard let url = URL(string: urlString) else {
            completion(.failure(FormatterError.invalidEndpoint))
            return
        }
        
        var request = URLRequest(url: url)
        request.httpMethod = "POST"
        request.timeoutInterval = 15
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        
        let body: [String: Any] = [
            "contents": [
                [
                    "parts": [
                        [
                            "inline_data": [
                                "mime_type": "audio/wav",
                                "data": base64Audio
                            ]
                        ],
                        [
                            "text": style.geminiPrompt
                        ]
                    ]
                ]
            ],
            "generationConfig": [
                "temperature": 0.0,
                "maxOutputTokens": 2048
            ]
        ]
        
        request.httpBody = try? JSONSerialization.data(withJSONObject: body)
        log("Gemini request: \(model) audio=\(audioData.count) bytes")
        
        URLSession.shared.dataTask(with: request) { data, response, error in
            if let error = error {
                log("Gemini API error: \(error)")
                completion(.failure(error))
                return
            }
            if let httpResponse = response as? HTTPURLResponse {
                log("Gemini API status: \(httpResponse.statusCode)")
            }
            guard let data = data else {
                log("Gemini API: no data")
                completion(.failure(FormatterError.noResponse))
                return
            }
            
            let rawResponse = String(data: data, encoding: .utf8) ?? "unreadable"
            log("Gemini API response: \(rawResponse.prefix(300))")
            
            // Parse Gemini response: candidates[0].content.parts[0].text
            guard let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
                  let candidates = json["candidates"] as? [[String: Any]],
                  let content = candidates.first?["content"] as? [String: Any],
                  let parts = content["parts"] as? [[String: Any]],
                  let responseText = parts.first?["text"] as? String else {
                log("Gemini response parse failed")
                // Check for error message
                if let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
                   let error = json["error"] as? [String: Any],
                   let message = error["message"] as? String {
                    log("Gemini error: \(message)")
                    completion(.failure(FormatterError.apiError(message)))
                } else {
                    completion(.failure(FormatterError.parseFailed))
                }
                return
            }
            
            // Try to parse as JSON {"text": "..."}
            let trimmed = responseText.trimmingCharacters(in: .whitespacesAndNewlines)
            
            // Strip markdown code fences if present
            var jsonString = trimmed
            if jsonString.hasPrefix("```json") {
                jsonString = String(jsonString.dropFirst(7))
            } else if jsonString.hasPrefix("```") {
                jsonString = String(jsonString.dropFirst(3))
            }
            if jsonString.hasSuffix("```") {
                jsonString = String(jsonString.dropLast(3))
            }
            jsonString = jsonString.trimmingCharacters(in: .whitespacesAndNewlines)
            
            if let jsonData = jsonString.data(using: .utf8),
               let parsed = try? JSONSerialization.jsonObject(with: jsonData) as? [String: String],
               let text = parsed["text"], !text.isEmpty {
                log("Gemini result: \"\(text.prefix(200))\"")
                completion(.success(text))
            } else {
                // Fallback: use raw response text (Gemini sometimes returns plain text)
                log("Gemini JSON parse failed, using raw text")
                completion(.success(trimmed))
            }
        }.resume()
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
                completion(.success(text))
                return
            }
            guard let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
                  let content = json["content"] as? [[String: Any]],
                  let textBlock = content.first?["text"] as? String else {
                completion(.success(text))
                return
            }
            let fullJSON = "{\(textBlock)}"
            if let innerData = fullJSON.data(using: .utf8),
               let innerJSON = try? JSONSerialization.jsonObject(with: innerData) as? [String: String],
               let cleaned = innerJSON["text"], !cleaned.isEmpty {
                completion(.success(cleaned))
            } else {
                let fallback = textBlock.trimmingCharacters(in: .whitespacesAndNewlines)
                completion(.success(fallback))
            }
        }.resume()
    }
}

enum FormatterError: LocalizedError {
    case invalidEndpoint
    case unsupportedProvider
    case audioReadFailed
    case noResponse
    case parseFailed
    case apiError(String)
    
    var errorDescription: String? {
        switch self {
        case .invalidEndpoint: return "Invalid API endpoint URL"
        case .unsupportedProvider: return "Provider does not support audio transcription"
        case .audioReadFailed: return "Failed to read audio file"
        case .noResponse: return "No response from API"
        case .parseFailed: return "Failed to parse API response"
        case .apiError(let msg): return "API error: \(msg)"
        }
    }
}
