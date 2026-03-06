import AVFoundation
import Accelerate

class AudioRecorder {
    private var engine: AVAudioEngine?
    private var audioFile: AVAudioFile?
    let tempURL = FileManager.default.temporaryDirectory
        .appendingPathComponent("yap_recording.wav")

    /// Called on main thread with per-band levels (array of 0.0-1.0) and overall RMS (0.0-1.0)
    var onLevelUpdate: ((Float) -> Void)?
    var onBandLevels: (([Float]) -> Void)?

    /// We compute 6 raw frequency bands, then mirror them for the 11-bar display
    private let rawBandCount = 6

    /// Start recording microphone audio to a temporary WAV file.
    /// Creates a fresh engine each call — caller should delay any sound playback
    /// until after start() returns, since engine.start() triggers aggregate device creation.
    func start() throws {
        let engine = AVAudioEngine()
        self.engine = engine
        let inputFormat = engine.inputNode.outputFormat(forBus: 0)

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

        engine.inputNode.installTap(onBus: 0, bufferSize: 2048, format: inputFormat) { [weak self] buffer, _ in
            guard let self = self else { return }
            try? self.audioFile?.write(from: buffer)

            guard let channelData = buffer.floatChannelData?[0] else { return }
            let frames = Int(buffer.frameLength)

            // RMS for overall level
            var sum: Float = 0
            for i in 0..<frames { sum += channelData[i] * channelData[i] }
            let rms = sqrtf(sum / Float(max(frames, 1)))
            let level = min(rms * 18.0, 1.0)

            // FFT for per-band levels, mirrored for symmetric display
            let rawBands = self.computeBands(channelData: channelData, frameCount: frames, sampleRate: Float(inputFormat.sampleRate))
            let mirrored = self.mirrorBands(rawBands)

            DispatchQueue.main.async {
                self.onLevelUpdate?(level)
                self.onBandLevels?(mirrored)
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
            return Array(repeating: 0, count: rawBandCount)
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
        
        var bands = [Float](repeating: 0, count: rawBandCount)
        for i in 0..<rawBandCount {
            let freqLow = pow(2.0, logMin + (logMax - logMin) * Float(i) / Float(rawBandCount))
            let freqHigh = pow(2.0, logMin + (logMax - logMin) * Float(i + 1) / Float(rawBandCount))
            let binLow = max(1, Int(freqLow / binWidth))
            let binHigh = min(fftSize / 2 - 1, Int(freqHigh / binWidth))
            
            if binHigh >= binLow {
                var sum: Float = 0
                for b in binLow...binHigh { sum += magnitudes[b] }
                bands[i] = sum / Float(binHigh - binLow + 1)
            }
        }
        
        // Get relative distribution (which bands are active vs others)
        let peak = bands.max() ?? 1
        if peak > 0 {
            for i in 0..<rawBandCount {
                bands[i] = bands[i] / peak
            }
        }
        
        // Compute overall RMS to use as a volume gate
        var rmsSum: Float = 0
        let rmsCount = min(frameCount, fftSize)
        for i in 0..<rmsCount { rmsSum += channelData[i] * channelData[i] }
        let rms = sqrtf(rmsSum / Float(max(rmsCount, 1)))
        // Aggressive scaling — normal speech should hit 0.6-0.9
        let volume = min(pow(rms * 18.0, 0.6), 1.0)
        
        // Multiply each band by volume — silence = all zero, speech = distributed
        for i in 0..<rawBandCount {
            bands[i] = bands[i] * volume
        }
        
        return bands
    }
    
    /// Mirror 6 raw bands into 11 display bars: center = band 0 (strongest), fanning out
    /// Outer bars blend in more of the neighboring bands so they're not starved
    private func mirrorBands(_ raw: [Float]) -> [Float] {
        guard raw.count >= 6 else { return Array(repeating: 0, count: 11) }
        // Outer bars get heavy blending from adjacent (more energetic) bands
        return [
            raw[5] * 0.5 + raw[4] * 0.3 + raw[3] * 0.2,  // bar 0  (leftmost)
            raw[4] * 0.5 + raw[3] * 0.3 + raw[5] * 0.2,  // bar 1
            raw[3] * 0.6 + raw[2] * 0.25 + raw[4] * 0.15, // bar 2
            raw[2] * 0.7 + raw[1] * 0.2 + raw[3] * 0.1,   // bar 3
            raw[1] * 0.8 + raw[0] * 0.15 + raw[2] * 0.05, // bar 4
            raw[0],                                          // bar 5  (center)
            raw[1] * 0.85 + raw[0] * 0.1 + raw[2] * 0.05, // bar 6
            raw[2] * 0.7 + raw[1] * 0.2 + raw[3] * 0.1,   // bar 7
            raw[3] * 0.6 + raw[2] * 0.25 + raw[4] * 0.15, // bar 8
            raw[4] * 0.5 + raw[3] * 0.3 + raw[5] * 0.2,   // bar 9
            raw[5] * 0.5 + raw[4] * 0.3 + raw[3] * 0.2,   // bar 10 (rightmost)
        ]
    }
}
