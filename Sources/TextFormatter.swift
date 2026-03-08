import Foundation

// MARK: - Enums

enum FormattingStyle: String, CaseIterable {
    case casual = "casual"
    case formatted = "formatted"
    case professional = "professional"
    
    var label: String {
        switch self {
        case .casual: return "Casual"
        case .formatted: return "Formatted"
        case .professional: return "Professional"
        }
    }
    
    var description: String {
        switch self {
        case .casual: return "Light cleanup, keeps your voice"
        case .formatted: return "Clean formatting, faithful to what you said"
        case .professional: return "Polished writing, elevated language"
        }
    }
    
    static let exampleInput = "um so like i was thinking we should probably you know move the meeting to friday because uh thursdays not gonna work for me"
    
    var exampleOutput: String {
        switch self {
        case .casual:
            return "so like i was thinking we should probably move the meeting to friday because thursdays not gonna work for me"
        case .formatted:
            return "So I was thinking we should probably move the meeting to Friday, because Thursday's not going to work for me."
        case .professional:
            return "I believe we should reschedule the meeting to Friday, as Thursday will not work for my schedule."
        }
    }
    
    /// Prompt for formatting already-transcribed text
    var prompt: String {
        switch self {
        case .casual:
            return """
            You clean up spoken text. You MUST respond with ONLY a JSON object: {"text":"cleaned version here"} \
            Rules: remove ONLY filler sounds (um, uh, er). Keep everything else exactly as spoken — \
            casual phrases, slang, sentence structure, contractions. All lowercase. Minimal punctuation. \
            PRESERVE all existing symbols — parentheses, quotes, brackets, etc. \
            Convert spoken punctuation commands to symbols (e.g. "period" → ., "open parenthesis" → (, "comma" → ,). \
            NEVER respond conversationally. ONLY output the JSON object.
            """
        case .formatted:
            return """
            You clean up spoken text. You MUST respond with ONLY a JSON object: {"text":"cleaned version here"} \
            Rules: remove filler words (um, uh, er, like, you know). Fix punctuation and capitalization. \
            Keep the speaker's EXACT words and sentence structure — do not rephrase or rewrite. \
            Keep contractions as spoken. Only fix obvious grammar errors. \
            PRESERVE all existing symbols — parentheses, quotes, brackets, etc. \
            Convert spoken punctuation commands to symbols (e.g. "period" → ., "open parenthesis" → (, "comma" → ,). \
            NEVER respond conversationally. ONLY output the JSON object.
            """
        case .professional:
            return """
            You clean up spoken text. You MUST respond with ONLY a JSON object: {"text":"cleaned version here"} \
            Rules: remove all filler words. Elevate the language to sound polished and professional. \
            Fix grammar, improve word choice, use proper punctuation and capitalization. \
            Expand contractions. You MAY rephrase for clarity and professionalism, but keep the original meaning. \
            PRESERVE all existing symbols — parentheses, quotes, brackets, etc. \
            Convert spoken punctuation commands to symbols (e.g. "period" → ., "open parenthesis" → (, "comma" → ,). \
            NEVER respond conversationally. ONLY output the JSON object.
            """
        }
    }
    
    /// Shared rules for all audio transcription prompts
    private static let noiseRule = "IGNORE all background noise, sound effects, music, and non-speech sounds. " +
        "Only transcribe human speech. If there is no speech, respond with {\"text\":\"\"}."
    
    private static let dictationCommands = """
        DICTATION COMMANDS — when the speaker says any of these, insert the symbol instead of the words: \
        "period" or "full stop" → . | "comma" → , | "question mark" → ? | "exclamation mark" or "exclamation point" → ! \
        "colon" → : | "semicolon" → ; | "open parenthesis" or "open paren" → ( | "close parenthesis" or "close paren" → ) \
        "open bracket" → [ | "close bracket" → ] | "open brace" or "open curly" → { | "close brace" or "close curly" → } \
        "open quote" or "open quotes" → " | "close quote" or "close quotes" or "end quote" → " \
        "dash" or "em dash" → — | "hyphen" → - | "ellipsis" or "dot dot dot" → … \
        "new line" or "newline" → insert a line break | "new paragraph" → insert two line breaks \
        "ampersand" → & | "at sign" → @ | "hashtag" or "hash" → # | "dollar sign" → $ | "percent" or "percent sign" → % \
        "asterisk" or "star" → * | "slash" or "forward slash" → / | "backslash" → \\ \
        "underscore" → _ | "pipe" → | | "tilde" → ~ | "caret" → ^ \
        Only convert these when the speaker clearly intends them as punctuation commands, not when used naturally in speech.
        """
    
    /// Prompt for multimodal transcription + formatting in one shot
    var audioPrompt: String {
        switch self {
        case .casual:
            return """
            Transcribe this audio. Remove filler sounds (um, uh, er) but keep everything else exactly as spoken — \
            casual phrases, slang, sentence structure, contractions. All lowercase. Minimal punctuation. \
            \(Self.dictationCommands) \
            \(Self.noiseRule) \
            You MUST respond with ONLY a JSON object: {"text":"transcription here"}
            """
        case .formatted:
            return """
            Transcribe this audio. Remove filler words (um, uh, er, like, you know). \
            Fix punctuation and capitalization. Keep the speaker's EXACT words and sentence structure — \
            do not rephrase or rewrite. Keep contractions as spoken. Only fix obvious grammar errors. \
            \(Self.dictationCommands) \
            \(Self.noiseRule) \
            You MUST respond with ONLY a JSON object: {"text":"transcription here"}
            """
        case .professional:
            return """
            Transcribe this audio. Remove all filler words. Elevate the language to sound polished and professional. \
            Fix grammar, improve word choice, use proper punctuation and capitalization. \
            Expand contractions. You MAY rephrase for clarity and professionalism, but keep the original meaning. \
            \(Self.dictationCommands) \
            \(Self.noiseRule) \
            You MUST respond with ONLY a JSON object: {"text":"transcription here"}
            """
        }
    }
    
    /// Plain transcription prompt (no formatting, for when formatting is handled separately)
    static let plainTranscriptionPrompt = """
        Transcribe this audio exactly as spoken, with proper punctuation and capitalization. \
        \(dictationCommands) \
        \(noiseRule) \
        You MUST respond with ONLY a JSON object: {"text":"transcription here"}
        """
}

enum TranscriptionProvider: String, CaseIterable {
    case none = "none"
    case gemini = "gemini"
    case openai = "openai"
    case deepgram = "deepgram"
    case elevenlabs = "elevenlabs"
    
    var label: String {
        switch self {
        case .none: return "None (Apple Dictation)"
        case .gemini: return "Google Gemini"
        case .openai: return "OpenAI"
        case .deepgram: return "Deepgram"
        case .elevenlabs: return "ElevenLabs"
        }
    }
    
    var defaultModel: String {
        switch self {
        case .none: return ""
        case .gemini: return "gemini-2.5-flash"
        case .openai: return "gpt-4o-transcribe"
        case .deepgram: return "nova-3"
        case .elevenlabs: return "scribe_v1"
        }
    }
    
    /// Whether this provider can also handle formatting (it's an LLM)
    var canAlsoFormat: Bool {
        switch self {
        case .gemini: return true
        default: return false
        }
    }
}

enum FormattingProvider: String, CaseIterable {
    case none = "none"
    case gemini = "gemini"
    case openai = "openai"
    case anthropic = "anthropic"
    case groq = "groq"

    var label: String {
        switch self {
        case .none: return "None"
        case .gemini: return "Google Gemini"
        case .openai: return "OpenAI"
        case .anthropic: return "Anthropic"
        case .groq: return "Groq"
        }
    }

    var defaultModel: String {
        switch self {
        case .none: return ""
        case .gemini: return "gemini-2.5-flash"
        case .openai: return "gpt-4o-mini"
        case .anthropic: return "claude-haiku-4-5-20251001"
        case .groq: return "llama-3.3-70b-versatile"
        }
    }
}

// MARK: - Provider Options

struct TranscriptionOptions {
    // Deepgram
    var dgSmartFormat: Bool = true
    var dgKeywords: [String] = []
    var dgLanguage: String = ""

    // OpenAI
    var oaiLanguage: String = ""
    var oaiPrompt: String = ""

    // Gemini
    var geminiTemperature: Double = 0.0

    // ElevenLabs
    var elLanguageCode: String = ""

    static func fromDefaults() -> TranscriptionOptions {
        let d = UserDefaults.standard
        var opts = TranscriptionOptions()
        opts.dgSmartFormat = d.object(forKey: SettingsKey.dgSmartFormat) as? Bool ?? true
        opts.dgLanguage = d.string(forKey: SettingsKey.dgLanguage) ?? ""
        let kw = d.string(forKey: SettingsKey.dgKeywords) ?? ""
        opts.dgKeywords = kw.split(separator: ",").map { $0.trimmingCharacters(in: .whitespaces) }.filter { !$0.isEmpty }
        opts.oaiLanguage = d.string(forKey: SettingsKey.oaiLanguage) ?? ""
        opts.oaiPrompt = d.string(forKey: SettingsKey.oaiPrompt) ?? ""
        opts.geminiTemperature = d.object(forKey: SettingsKey.geminiTemperature) as? Double ?? 0.0
        opts.elLanguageCode = d.string(forKey: SettingsKey.elLanguageCode) ?? ""
        return opts
    }
}

// MARK: - Transcriber (API-based)

class AudioTranscriber {
    let provider: TranscriptionProvider
    private let apiKey: String
    private let model: String
    let options: TranscriptionOptions

    init(provider: TranscriptionProvider, apiKey: String, model: String? = nil, options: TranscriptionOptions = .fromDefaults()) {
        self.provider = provider
        self.apiKey = apiKey
        self.model = model ?? provider.defaultModel
        self.options = options
    }
    
    /// Maximum number of retries for transient failures (timeouts, truncated responses)
    private static let maxRetries = 2
    
    /// Transcribe audio, optionally doing formatting in one shot (Gemini only)
    func transcribe(audioURL: URL, style: FormattingStyle? = nil, completion: @escaping (Result<String, Error>) -> Void) {
        guard let audioData = try? Data(contentsOf: audioURL) else {
            completion(.failure(FormatterError.audioReadFailed))
            return
        }
        
        // Scale timeout with audio length: ~30s base + 1s per second of audio
        // 16-bit PCM WAV at typical sample rates ≈ 32KB/s (mono 16kHz) to 96KB/s (stereo 48kHz)
        let estimatedSeconds = Double(audioData.count) / 64_000  // conservative middle estimate
        let timeout = max(30.0, 30.0 + estimatedSeconds)
        
        log("Transcribing with \(provider.rawValue), model=\(model), audio=\(audioData.count) bytes, timeout=\(String(format: "%.0f", timeout))s")
        
        transcribeWithRetry(audioData: audioData, style: style, timeout: timeout, attempt: 1, completion: completion)
    }
    
    /// Internal retry wrapper — retries on timeout or truncated response
    private func transcribeWithRetry(audioData: Data, style: FormattingStyle?, timeout: TimeInterval, attempt: Int, completion: @escaping (Result<String, Error>) -> Void) {
        let singleAttempt: (@escaping (Result<String, Error>) -> Void) -> Void = { cb in
            switch self.provider {
            case .none:
                cb(.failure(FormatterError.unsupportedProvider))
            case .gemini:
                self.callGemini(audioData: audioData, style: style, timeout: timeout, completion: cb)
            case .openai:
                self.callOpenAITranscribe(audioData: audioData, timeout: timeout, completion: cb)
            case .deepgram:
                self.callDeepgram(audioData: audioData, timeout: timeout, completion: cb)
            case .elevenlabs:
                self.callElevenLabs(audioData: audioData, timeout: timeout, completion: cb)
            }
        }
        
        singleAttempt { result in
            switch result {
            case .success:
                completion(result)
            case .failure(let error):
                let isRetryable: Bool
                if error is FormatterError {
                    switch error as! FormatterError {
                    case .truncatedResponse, .noResponse, .parseFailed:
                        isRetryable = true
                    default:
                        isRetryable = false
                    }
                } else {
                    // URLSession timeout errors
                    isRetryable = (error as NSError).code == NSURLErrorTimedOut
                        || (error as NSError).code == NSURLErrorNetworkConnectionLost
                }
                
                if isRetryable && attempt < Self.maxRetries {
                    log("⚠️ Attempt \(attempt) failed (\(error.localizedDescription)), retrying (\(attempt + 1)/\(Self.maxRetries))...")
                    // Small backoff before retry
                    DispatchQueue.global().asyncAfter(deadline: .now() + Double(attempt) * 0.5) {
                        self.transcribeWithRetry(audioData: audioData, style: style, timeout: timeout, attempt: attempt + 1, completion: completion)
                    }
                } else {
                    completion(result)
                }
            }
        }
    }
    
    // MARK: Gemini
    
    private func callGemini(audioData: Data, style: FormattingStyle?, timeout: TimeInterval, completion: @escaping (Result<String, Error>) -> Void) {
        let base64Audio = audioData.base64EncodedString()
        let prompt = style?.audioPrompt ?? FormattingStyle.plainTranscriptionPrompt
        let urlString = "https://generativelanguage.googleapis.com/v1beta/models/\(model):generateContent?key=\(apiKey)"
        
        guard let url = URL(string: urlString) else {
            completion(.failure(FormatterError.invalidEndpoint))
            return
        }
        
        var request = URLRequest(url: url)
        request.httpMethod = "POST"
        request.timeoutInterval = timeout
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        
        let body: [String: Any] = [
            "contents": [[
                "parts": [
                    ["inline_data": ["mime_type": "audio/wav", "data": base64Audio]],
                    ["text": prompt]
                ]
            ]],
            "generationConfig": ["temperature": options.geminiTemperature, "maxOutputTokens": 2048, "responseMimeType": "application/json"]
        ]

        request.httpBody = try? JSONSerialization.data(withJSONObject: body)

        makeRequest(request, label: "Gemini") { data in
            guard let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
                  let candidates = json["candidates"] as? [[String: Any]],
                  let candidate = candidates.first else {
                if let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
                   let err = json["error"] as? [String: Any],
                   let msg = err["message"] as? String {
                    return .failure(FormatterError.apiError(msg))
                }
                return .failure(FormatterError.parseFailed)
            }

            // Check finishReason — anything other than STOP means truncated/blocked
            let finishReason = candidate["finishReason"] as? String ?? "UNKNOWN"
            if finishReason != "STOP" {
                log("⚠️ Gemini finishReason: \(finishReason) (expected STOP)")
                return .failure(FormatterError.truncatedResponse(reason: finishReason))
            }

            guard let content = candidate["content"] as? [String: Any],
                  let parts = content["parts"] as? [[String: Any]],
                  let text = parts.first?["text"] as? String else {
                return .failure(FormatterError.parseFailed)
            }

            return .success(self.extractJSON(from: text))
        } completion: { completion($0) }
    }
    
    // MARK: OpenAI
    
    private func callOpenAITranscribe(audioData: Data, timeout: TimeInterval, completion: @escaping (Result<String, Error>) -> Void) {
        guard let url = URL(string: "https://api.openai.com/v1/audio/transcriptions") else {
            completion(.failure(FormatterError.invalidEndpoint))
            return
        }

        var fields = ["model": model]
        if !options.oaiLanguage.isEmpty { fields["language"] = options.oaiLanguage }
        if !options.oaiPrompt.isEmpty { fields["prompt"] = options.oaiPrompt }

        let boundary = UUID().uuidString
        var request = URLRequest(url: url)
        request.httpMethod = "POST"
        request.timeoutInterval = timeout
        request.setValue("Bearer \(apiKey)", forHTTPHeaderField: "Authorization")
        request.setValue("multipart/form-data; boundary=\(boundary)", forHTTPHeaderField: "Content-Type")
        request.httpBody = multipartBody(boundary: boundary, audioData: audioData, fields: fields)
        
        makeRequest(request, label: "OpenAI") { data in
            if let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
               let text = json["text"] as? String {
                return .success(text)
            }
            if let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
               let err = json["error"] as? [String: Any],
               let msg = err["message"] as? String {
                return .failure(FormatterError.apiError(msg))
            }
            return .failure(FormatterError.parseFailed)
        } completion: { completion($0) }
    }
    
    // MARK: Deepgram
    
    private func callDeepgram(audioData: Data, timeout: TimeInterval, completion: @escaping (Result<String, Error>) -> Void) {
        var params = ["model=\(model)"]
        if options.dgSmartFormat { params.append("smart_format=true") }
        if !options.dgLanguage.isEmpty { params.append("language=\(options.dgLanguage)") }
        for kw in options.dgKeywords {
            params.append("keywords=\(kw.addingPercentEncoding(withAllowedCharacters: .urlQueryAllowed) ?? kw)")
        }

        guard let url = URL(string: "https://api.deepgram.com/v1/listen?\(params.joined(separator: "&"))") else {
            completion(.failure(FormatterError.invalidEndpoint))
            return
        }

        var request = URLRequest(url: url)
        request.httpMethod = "POST"
        request.timeoutInterval = timeout
        request.setValue("Token \(apiKey)", forHTTPHeaderField: "Authorization")
        request.setValue("audio/wav", forHTTPHeaderField: "Content-Type")
        request.httpBody = audioData

        makeRequest(request, label: "Deepgram") { data in
            if let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
               let results = json["results"] as? [String: Any],
               let channels = results["channels"] as? [[String: Any]],
               let alts = channels.first?["alternatives"] as? [[String: Any]],
               let transcript = alts.first?["transcript"] as? String {
                return .success(transcript)
            }
            if let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
               let msg = json["err_msg"] as? String {
                return .failure(FormatterError.apiError(msg))
            }
            return .failure(FormatterError.parseFailed)
        } completion: { completion($0) }
    }
    
    // MARK: ElevenLabs
    
    private func callElevenLabs(audioData: Data, timeout: TimeInterval, completion: @escaping (Result<String, Error>) -> Void) {
        guard let url = URL(string: "https://api.elevenlabs.io/v1/speech-to-text") else {
            completion(.failure(FormatterError.invalidEndpoint))
            return
        }
        
        let boundary = UUID().uuidString
        var request = URLRequest(url: url)
        request.httpMethod = "POST"
        request.timeoutInterval = timeout
        request.setValue(apiKey, forHTTPHeaderField: "xi-api-key")
        request.setValue("multipart/form-data; boundary=\(boundary)", forHTTPHeaderField: "Content-Type")
        var fields = ["model_id": model]
        if !options.elLanguageCode.isEmpty { fields["language_code"] = options.elLanguageCode }
        request.httpBody = multipartBody(boundary: boundary, audioData: audioData, fields: fields)

        makeRequest(request, label: "ElevenLabs") { data in
            if let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
               let text = json["text"] as? String {
                return .success(text)
            }
            if let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
               let detail = json["detail"] as? [String: Any],
               let msg = detail["message"] as? String {
                return .failure(FormatterError.apiError(msg))
            }
            return .failure(FormatterError.parseFailed)
        } completion: { completion($0) }
    }
    
    // MARK: Helpers
    
    private func makeRequest(_ request: URLRequest, label: String,
                             parse: @escaping (Data) -> Result<String, Error>,
                             completion: @escaping (Result<String, Error>) -> Void) {
        URLSession.shared.dataTask(with: request) { data, response, error in
            if let error = error {
                log("\(label) error: \(error)")
                completion(.failure(error))
                return
            }
            if let http = response as? HTTPURLResponse {
                log("\(label) status: \(http.statusCode)")
            }
            guard let data = data else {
                completion(.failure(FormatterError.noResponse))
                return
            }
            log("\(label) response: \(String(data: data, encoding: .utf8)?.prefix(300) ?? "unreadable")")
            completion(parse(data))
        }.resume()
    }
    
    private func multipartBody(boundary: String, audioData: Data, fields: [String: String]) -> Data {
        var body = Data()
        body.append("--\(boundary)\r\n")
        body.append("Content-Disposition: form-data; name=\"file\"; filename=\"recording.wav\"\r\n")
        body.append("Content-Type: audio/wav\r\n\r\n")
        body.append(audioData)
        body.append("\r\n")
        for (key, value) in fields {
            body.append("--\(boundary)\r\n")
            body.append("Content-Disposition: form-data; name=\"\(key)\"\r\n\r\n")
            body.append("\(value)\r\n")
        }
        body.append("--\(boundary)--\r\n")
        return body
    }
    
    private func extractJSON(from text: String) -> String {
        var s = text.trimmingCharacters(in: .whitespacesAndNewlines)
        // Strip markdown code fences
        if s.hasPrefix("```json") { s = String(s.dropFirst(7)) }
        else if s.hasPrefix("```") { s = String(s.dropFirst(3)) }
        if s.hasSuffix("```") { s = String(s.dropLast(3)) }
        s = s.trimmingCharacters(in: .whitespacesAndNewlines)
        // Try direct JSON parse
        if let data = s.data(using: .utf8),
           let parsed = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
           let text = parsed["text"] as? String {
            return text
        }
        // Try to find JSON object anywhere in the string
        if let start = s.range(of: "{"), let end = s.range(of: "}", options: .backwards) {
            let jsonSlice = String(s[start.lowerBound...end.upperBound])
            if let data = jsonSlice.data(using: .utf8),
               let parsed = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
               let text = parsed["text"] as? String {
                return text
            }
        }
        return s
    }
}

// MARK: - Text Formatter (LLM-based formatting)

class TextFormatter {
    let provider: FormattingProvider
    private let apiKey: String
    private let model: String
    private let style: FormattingStyle
    
    init(provider: FormattingProvider, apiKey: String, model: String? = nil, style: FormattingStyle) {
        self.provider = provider
        self.apiKey = apiKey
        self.model = model ?? provider.defaultModel
        self.style = style
    }
    
    func format(_ text: String, completion: @escaping (Result<String, Error>) -> Void) {
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard trimmed.count >= 3, !apiKey.isEmpty else {
            completion(.success(text))
            return
        }
        
        log("Formatting with \(provider.rawValue), model=\(model), style=\(style.rawValue)")
        
        switch provider {
        case .none:
            completion(.success(text))
        case .gemini:
            callGemini(text: text, completion: completion)
        case .openai:
            callOpenAI(text: text, completion: completion)
        case .anthropic:
            callAnthropic(text: text, completion: completion)
        case .groq:
            callGroq(text: text, completion: completion)
        }
    }
    
    // MARK: Gemini (text → formatted text)
    
    private func callGemini(text: String, completion: @escaping (Result<String, Error>) -> Void) {
        let urlString = "https://generativelanguage.googleapis.com/v1beta/models/\(model):generateContent?key=\(apiKey)"
        guard let url = URL(string: urlString) else {
            completion(.failure(FormatterError.invalidEndpoint))
            return
        }
        
        var request = URLRequest(url: url)
        request.httpMethod = "POST"
        request.timeoutInterval = 15
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")

        let body: [String: Any] = [
            "contents": [[
                "parts": [["text": "\(style.prompt)\n\n<input>\(text)</input>"]]
            ]],
            "generationConfig": ["temperature": 0.0, "maxOutputTokens": 2048, "responseMimeType": "application/json"]
        ]
        
        request.httpBody = try? JSONSerialization.data(withJSONObject: body)
        
        URLSession.shared.dataTask(with: request) { data, response, error in
            if let error = error { completion(.failure(error)); return }
            guard let data = data,
                  let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
                  let candidates = json["candidates"] as? [[String: Any]],
                  let candidate = candidates.first else {
                completion(.success(text))  // fall back to unformatted
                return
            }
            
            // Check finishReason — truncated formatting isn't usable
            let finishReason = candidate["finishReason"] as? String ?? "UNKNOWN"
            if finishReason != "STOP" {
                log("⚠️ Gemini format finishReason: \(finishReason) — falling back to raw text")
                completion(.success(text))
                return
            }
            
            guard let content = candidate["content"] as? [String: Any],
                  let parts = content["parts"] as? [[String: Any]],
                  let responseText = parts.first?["text"] as? String else {
                completion(.success(text))
                return
            }
            completion(.success(Self.extractJSON(from: responseText)))
        }.resume()
    }
    
    // MARK: OpenAI
    
    private func callOpenAI(text: String, completion: @escaping (Result<String, Error>) -> Void) {
        guard let url = URL(string: "https://api.openai.com/v1/chat/completions") else {
            completion(.failure(FormatterError.invalidEndpoint))
            return
        }
        
        var request = URLRequest(url: url)
        request.httpMethod = "POST"
        request.timeoutInterval = 15
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
            if let error = error { completion(.failure(error)); return }
            guard let data = data,
                  let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
                  let choices = json["choices"] as? [[String: Any]],
                  let message = choices.first?["message"] as? [String: Any],
                  let content = message["content"] as? String else {
                completion(.success(text))
                return
            }
            completion(.success(Self.extractJSON(from: content)))
        }.resume()
    }
    
    // MARK: Anthropic
    
    private func callAnthropic(text: String, completion: @escaping (Result<String, Error>) -> Void) {
        guard let url = URL(string: "https://api.anthropic.com/v1/messages") else {
            completion(.failure(FormatterError.invalidEndpoint))
            return
        }
        
        var request = URLRequest(url: url)
        request.httpMethod = "POST"
        request.timeoutInterval = 15
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
            if let error = error { completion(.failure(error)); return }
            guard let data = data,
                  let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
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
                completion(.success(textBlock.trimmingCharacters(in: .whitespacesAndNewlines)))
            }
        }.resume()
    }
    
    // MARK: Groq

    private func callGroq(text: String, completion: @escaping (Result<String, Error>) -> Void) {
        guard let url = URL(string: "https://api.groq.com/openai/v1/chat/completions") else {
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
            if let error = error { completion(.failure(error)); return }
            guard let data = data,
                  let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
                  let choices = json["choices"] as? [[String: Any]],
                  let message = choices.first?["message"] as? [String: Any],
                  let content = message["content"] as? String else {
                completion(.success(text))
                return
            }
            completion(.success(Self.extractJSON(from: content)))
        }.resume()
    }

    // MARK: Helpers

    static func extractJSON(from text: String) -> String {
        var s = text.trimmingCharacters(in: .whitespacesAndNewlines)
        // Strip markdown code fences
        if s.hasPrefix("```json") { s = String(s.dropFirst(7)) }
        else if s.hasPrefix("```") { s = String(s.dropFirst(3)) }
        if s.hasSuffix("```") { s = String(s.dropLast(3)) }
        s = s.trimmingCharacters(in: .whitespacesAndNewlines)
        // Try direct JSON parse
        if let data = s.data(using: .utf8),
           let parsed = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
           let text = parsed["text"] as? String {
            return text
        }
        // Try to find JSON object anywhere in the string
        if let start = s.range(of: "{"), let end = s.range(of: "}", options: .backwards) {
            let jsonSlice = String(s[start.lowerBound...end.upperBound])
            if let data = jsonSlice.data(using: .utf8),
               let parsed = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
               let text = parsed["text"] as? String {
                return text
            }
        }
        return s
    }
}

// MARK: - Data extension

extension Data {
    mutating func append(_ string: String) {
        if let data = string.data(using: .utf8) { append(data) }
    }
}

// MARK: - Errors

enum FormatterError: LocalizedError {
    case invalidEndpoint, unsupportedProvider, audioReadFailed, noResponse, parseFailed
    case apiError(String)
    case truncatedResponse(reason: String)
    
    var errorDescription: String? {
        switch self {
        case .invalidEndpoint: return "Invalid API endpoint URL"
        case .unsupportedProvider: return "Provider does not support this operation"
        case .audioReadFailed: return "Failed to read audio file"
        case .noResponse: return "No response from API"
        case .parseFailed: return "Failed to parse API response"
        case .apiError(let msg): return "API error: \(msg)"
        case .truncatedResponse(let reason): return "Response truncated (finishReason: \(reason))"
        }
    }
}
