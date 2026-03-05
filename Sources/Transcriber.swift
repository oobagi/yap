import Speech

class Transcriber {
    private let recognizer: SFSpeechRecognizer
    
    init(locale: Locale = Locale(identifier: "en-US")) {
        guard let recognizer = SFSpeechRecognizer(locale: locale) else {
            fatalError("Speech recognition not available for locale: \(locale.identifier)")
        }
        self.recognizer = recognizer
    }
    
    /// Request speech recognition permission. Call once at launch.
    static func requestAuthorization(completion: @escaping (Bool) -> Void) {
        SFSpeechRecognizer.requestAuthorization { status in
            DispatchQueue.main.async {
                completion(status == .authorized)
            }
        }
    }
    
    /// Transcribe a local audio file.
    func transcribe(audioURL: URL, completion: @escaping (Result<String, Error>) -> Void) {
        guard recognizer.isAvailable else {
            completion(.failure(TranscriptionError.unavailable))
            return
        }
        
        let request = SFSpeechURLRecognitionRequest(url: audioURL)
        request.shouldReportPartialResults = false
        
        // Use on-device when available (macOS 13+), falls back to server
        if #available(macOS 13.0, *) {
            request.requiresOnDeviceRecognition = false
            request.addsPunctuation = true
        }
        
        recognizer.recognitionTask(with: request) { result, error in
            // recognitionTask can call back multiple times — only act on final
            if let error = error {
                completion(.failure(error))
                return
            }
            
            guard let result = result, result.isFinal else { return }
            completion(.success(result.bestTranscription.formattedString))
        }
    }
}

enum TranscriptionError: LocalizedError {
    case unavailable
    
    var errorDescription: String? {
        switch self {
        case .unavailable:
            return "Speech recognition is not currently available"
        }
    }
}
