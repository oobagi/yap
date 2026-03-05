import AVFoundation

class AudioRecorder {
    private var engine: AVAudioEngine?
    private var audioFile: AVAudioFile?
    let tempURL = FileManager.default.temporaryDirectory
        .appendingPathComponent("voicetype_recording.wav")
    
    /// Start recording microphone audio to a temporary WAV file.
    func start() throws {
        let engine = AVAudioEngine()
        self.engine = engine
        
        let inputNode = engine.inputNode
        let inputFormat = inputNode.outputFormat(forBus: 0)
        
        // Write as 16-bit PCM WAV
        let settings: [String: Any] = [
            AVFormatIDKey: kAudioFormatLinearPCM,
            AVSampleRateKey: inputFormat.sampleRate,
            AVNumberOfChannelsKey: inputFormat.channelCount,
            AVLinearPCMBitDepthKey: 16,
            AVLinearPCMIsFloatKey: false,
            AVLinearPCMIsBigEndianKey: false,
        ]
        
        audioFile = try AVAudioFile(forWriting: tempURL, settings: settings)
        
        inputNode.installTap(onBus: 0, bufferSize: 4096, format: inputFormat) { [weak self] buffer, _ in
            try? self?.audioFile?.write(from: buffer)
        }
        
        engine.prepare()
        try engine.start()
    }
    
    /// Stop recording and return the audio file URL. Returns nil on failure.
    func stop() -> URL? {
        engine?.inputNode.removeTap(onBus: 0)
        engine?.stop()
        engine = nil
        audioFile = nil // flush & close
        
        guard FileManager.default.fileExists(atPath: tempURL.path) else { return nil }
        return tempURL
    }
    
    /// Cancel recording without returning data.
    func cancel() {
        engine?.inputNode.removeTap(onBus: 0)
        engine?.stop()
        engine = nil
        audioFile = nil
    }
}
