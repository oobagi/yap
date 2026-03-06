import AVFoundation

class AudioRecorder {
    private var engine: AVAudioEngine?
    private var audioFile: AVAudioFile?
    let tempURL = FileManager.default.temporaryDirectory
        .appendingPathComponent("voicetype_recording.wav")
    
    /// Called on main thread with current audio level (0.0 - 1.0)
    var onLevelUpdate: ((Float) -> Void)?
    
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
        
        inputNode.installTap(onBus: 0, bufferSize: 1024, format: inputFormat) { [weak self] buffer, _ in
            try? self?.audioFile?.write(from: buffer)
            
            // Calculate RMS level from buffer
            guard let channelData = buffer.floatChannelData?[0] else { return }
            let frames = Int(buffer.frameLength)
            var sum: Float = 0
            for i in 0..<frames {
                sum += channelData[i] * channelData[i]
            }
            let rms = sqrtf(sum / Float(max(frames, 1)))
            // Normalize to 0-1 range — aggressive scaling so normal speech fills the bars
            let level = min(rms * 18.0, 1.0)
            
            DispatchQueue.main.async {
                self?.onLevelUpdate?(level)
            }
        }
        
        engine.prepare()
        try engine.start()
    }
    
    /// Stop recording and return the audio file URL. Returns nil on failure.
    func stop() -> URL? {
        engine?.inputNode.removeTap(onBus: 0)
        engine?.stop()
        engine = nil
        audioFile = nil
        return FileManager.default.fileExists(atPath: tempURL.path) ? tempURL : nil
    }
    
    /// Cancel recording without returning data.
    func cancel() {
        engine?.inputNode.removeTap(onBus: 0)
        engine?.stop()
        engine = nil
        audioFile = nil
    }
}
