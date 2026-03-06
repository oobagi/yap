import AVFoundation
import Accelerate

class AudioRecorder {
    private var engine: AVAudioEngine?
    private var audioFile: AVAudioFile?
    let tempURL = FileManager.default.temporaryDirectory
        .appendingPathComponent("voicetype_recording.wav")
    
    /// Called on main thread with per-band levels (array of 0.0-1.0) and overall RMS (0.0-1.0)
    var onLevelUpdate: ((Float) -> Void)?
    var onBandLevels: (([Float]) -> Void)?
    
    private let bandCount = 11
    
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
        
        inputNode.installTap(onBus: 0, bufferSize: 2048, format: inputFormat) { [weak self] buffer, _ in
            guard let self = self else { return }
            try? self.audioFile?.write(from: buffer)
            
            guard let channelData = buffer.floatChannelData?[0] else { return }
            let frames = Int(buffer.frameLength)
            
            // RMS for overall level
            var sum: Float = 0
            for i in 0..<frames { sum += channelData[i] * channelData[i] }
            let rms = sqrtf(sum / Float(max(frames, 1)))
            let level = min(rms * 18.0, 1.0)
            
            // FFT for per-band levels
            let bands = self.computeBands(channelData: channelData, frameCount: frames, sampleRate: Float(inputFormat.sampleRate))
            
            DispatchQueue.main.async {
                self.onLevelUpdate?(level)
                self.onBandLevels?(bands)
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
    
    /// Compute frequency band levels using FFT via Accelerate
    private func computeBands(channelData: UnsafeMutablePointer<Float>, frameCount: Int, sampleRate: Float) -> [Float] {
        // Use power-of-2 FFT size
        let fftSize = 1024
        let log2n = vDSP_Length(log2(Float(fftSize)))
        guard let fftSetup = vDSP_create_fftsetup(log2n, FFTRadix(kFFTRadix2)) else {
            return Array(repeating: 0, count: bandCount)
        }
        defer { vDSP_destroy_fftsetup(fftSetup) }
        
        // Apply Hann window
        var windowed = [Float](repeating: 0, count: fftSize)
        var window = [Float](repeating: 0, count: fftSize)
        vDSP_hann_window(&window, vDSP_Length(fftSize), Int32(vDSP_HANN_NORM))
        let count = min(frameCount, fftSize)
        for i in 0..<count { windowed[i] = channelData[i] * window[i] }
        
        // Split complex
        var realp = [Float](repeating: 0, count: fftSize / 2)
        var imagp = [Float](repeating: 0, count: fftSize / 2)
        realp.withUnsafeMutableBufferPointer { realBuf in
            imagp.withUnsafeMutableBufferPointer { imagBuf in
                var splitComplex = DSPSplitComplex(realp: realBuf.baseAddress!, imagp: imagBuf.baseAddress!)
                windowed.withUnsafeBufferPointer { winBuf in
                    winBuf.baseAddress!.withMemoryRebound(to: DSPComplex.self, capacity: fftSize / 2) { ptr in
                        vDSP_ctoz(ptr, 2, &splitComplex, 1, vDSP_Length(fftSize / 2))
                    }
                }
                vDSP_fft_zrip(fftSetup, &splitComplex, 1, log2n, FFTDirection(FFT_FORWARD))
            }
        }
        
        // Compute magnitudes
        var magnitudes = [Float](repeating: 0, count: fftSize / 2)
        realp.withUnsafeMutableBufferPointer { realBuf in
            imagp.withUnsafeMutableBufferPointer { imagBuf in
                var splitComplex = DSPSplitComplex(realp: realBuf.baseAddress!, imagp: imagBuf.baseAddress!)
                vDSP_zvmags(&splitComplex, 1, &magnitudes, 1, vDSP_Length(fftSize / 2))
            }
        }
        
        // Logarithmic frequency bands (voice range ~80Hz - 8kHz)
        let nyquist = sampleRate / 2
        let binWidth = nyquist / Float(fftSize / 2)
        let minFreq: Float = 80
        let maxFreq: Float = min(8000, nyquist)
        let logMin = log2(minFreq)
        let logMax = log2(maxFreq)
        
        var bands = [Float](repeating: 0, count: bandCount)
        for i in 0..<bandCount {
            let freqLow = pow(2.0, logMin + (logMax - logMin) * Float(i) / Float(bandCount))
            let freqHigh = pow(2.0, logMin + (logMax - logMin) * Float(i + 1) / Float(bandCount))
            let binLow = max(1, Int(freqLow / binWidth))
            let binHigh = min(fftSize / 2 - 1, Int(freqHigh / binWidth))
            
            if binHigh >= binLow {
                var sum: Float = 0
                for b in binLow...binHigh { sum += magnitudes[b] }
                let avg = sum / Float(binHigh - binLow + 1)
                // Convert to dB-ish scale, normalize aggressively
                let db = 10 * log10(max(avg, 1e-10))
                bands[i] = min(1.0, max(0.0, (db + 50) / 40))
            }
        }
        
        return bands
    }
}
