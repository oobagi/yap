using System;

namespace Yap.Audio
{
    /// <summary>
    /// Performs 1024-point FFT with Hann windowing to compute 6 logarithmic frequency bands
    /// (80Hz - 8kHz), then mirrors them to 11 display bars.
    /// Direct port of AudioRecorder.computeBands() and mirrorBands() from the macOS version.
    /// </summary>
    public class FftProcessor
    {
        private const int FftSize = 1024;
        private const int RawBandCount = 6;
        private readonly float[] _hannWindow;

        public FftProcessor()
        {
            // Pre-compute Hann window
            _hannWindow = new float[FftSize];
            for (int i = 0; i < FftSize; i++)
            {
                _hannWindow[i] = 0.5f * (1.0f - MathF.Cos(2.0f * MathF.PI * i / FftSize));
            }
        }

        /// <summary>
        /// Compute 6 logarithmic frequency band levels from audio samples.
        /// Each band is volume-gated (silent audio = zero bands).
        /// </summary>
        public float[] ComputeBands(float[] channelData, float sampleRate)
        {
            var bands = new float[RawBandCount];
            if (channelData.Length == 0) return bands;

            // Apply Hann window
            var windowed = new float[FftSize];
            int count = Math.Min(channelData.Length, FftSize);
            for (int i = 0; i < count; i++)
            {
                windowed[i] = channelData[i] * _hannWindow[i];
            }

            // Perform FFT (in-place, Cooley-Tukey radix-2)
            var real = new float[FftSize];
            var imag = new float[FftSize];
            Array.Copy(windowed, real, FftSize);

            Fft(real, imag);

            // Compute magnitudes (only need first half due to symmetry)
            int halfSize = FftSize / 2;
            var magnitudes = new float[halfSize];
            for (int i = 0; i < halfSize; i++)
            {
                magnitudes[i] = real[i] * real[i] + imag[i] * imag[i];
            }

            // Logarithmic frequency bands (voice range ~80Hz - 8kHz)
            float nyquist = sampleRate / 2.0f;
            float binWidth = nyquist / halfSize;
            float minFreq = 80.0f;
            float maxFreq = Math.Min(8000.0f, nyquist);
            float logMin = MathF.Log2(minFreq);
            float logMax = MathF.Log2(maxFreq);

            for (int i = 0; i < RawBandCount; i++)
            {
                float freqLow = MathF.Pow(2.0f, logMin + (logMax - logMin) * i / RawBandCount);
                float freqHigh = MathF.Pow(2.0f, logMin + (logMax - logMin) * (i + 1) / RawBandCount);
                int binLow = Math.Max(1, (int)(freqLow / binWidth));
                int binHigh = Math.Min(halfSize - 1, (int)(freqHigh / binWidth));

                if (binHigh >= binLow)
                {
                    float sum = 0;
                    for (int b = binLow; b <= binHigh; b++)
                    {
                        sum += magnitudes[b];
                    }
                    bands[i] = sum / (binHigh - binLow + 1);
                }
            }

            // Normalize to relative distribution
            float peak = 0;
            for (int i = 0; i < RawBandCount; i++)
            {
                if (bands[i] > peak) peak = bands[i];
            }
            if (peak > 0)
            {
                for (int i = 0; i < RawBandCount; i++)
                {
                    bands[i] /= peak;
                }
            }

            // Volume gate: multiply by RMS-based volume
            float rmsSum = 0;
            int rmsCount = Math.Min(channelData.Length, FftSize);
            for (int i = 0; i < rmsCount; i++)
            {
                rmsSum += channelData[i] * channelData[i];
            }
            float rms = MathF.Sqrt(rmsSum / Math.Max(rmsCount, 1));
            // Aggressive scaling - normal speech should hit 0.6-0.9
            float volume = Math.Min(MathF.Pow(rms * 18.0f, 0.6f), 1.0f);

            for (int i = 0; i < RawBandCount; i++)
            {
                bands[i] *= volume;
            }

            return bands;
        }

        /// <summary>
        /// Mirror 6 raw bands into 11 display bars.
        /// Center = band 0 (strongest), fanning out.
        /// Outer bars blend neighboring bands so they're not starved.
        /// </summary>
        public float[] MirrorBands(float[] raw)
        {
            if (raw.Length < 6)
            {
                return new float[11];
            }

            return new[]
            {
                raw[5] * 0.5f + raw[4] * 0.3f + raw[3] * 0.2f,   // bar 0  (leftmost)
                raw[4] * 0.5f + raw[3] * 0.3f + raw[5] * 0.2f,   // bar 1
                raw[3] * 0.6f + raw[2] * 0.25f + raw[4] * 0.15f,  // bar 2
                raw[2] * 0.7f + raw[1] * 0.2f + raw[3] * 0.1f,    // bar 3
                raw[1] * 0.8f + raw[0] * 0.15f + raw[2] * 0.05f,  // bar 4
                raw[0],                                              // bar 5  (center)
                raw[1] * 0.85f + raw[0] * 0.1f + raw[2] * 0.05f,  // bar 6
                raw[2] * 0.7f + raw[1] * 0.2f + raw[3] * 0.1f,    // bar 7
                raw[3] * 0.6f + raw[2] * 0.25f + raw[4] * 0.15f,  // bar 8
                raw[4] * 0.5f + raw[3] * 0.3f + raw[5] * 0.2f,    // bar 9
                raw[5] * 0.5f + raw[4] * 0.3f + raw[3] * 0.2f,    // bar 10 (rightmost)
            };
        }

        /// <summary>
        /// In-place Cooley-Tukey radix-2 FFT.
        /// </summary>
        private static void Fft(float[] real, float[] imag)
        {
            int n = real.Length;
            if (n <= 1) return;

            // Bit-reversal permutation
            int j = 0;
            for (int i = 0; i < n - 1; i++)
            {
                if (i < j)
                {
                    (real[i], real[j]) = (real[j], real[i]);
                    (imag[i], imag[j]) = (imag[j], imag[i]);
                }
                int k = n >> 1;
                while (k <= j)
                {
                    j -= k;
                    k >>= 1;
                }
                j += k;
            }

            // Cooley-Tukey butterfly
            for (int len = 2; len <= n; len <<= 1)
            {
                float angle = -2.0f * MathF.PI / len;
                float wReal = MathF.Cos(angle);
                float wImag = MathF.Sin(angle);

                for (int i = 0; i < n; i += len)
                {
                    float curReal = 1.0f;
                    float curImag = 0.0f;

                    for (int k = 0; k < len / 2; k++)
                    {
                        int u = i + k;
                        int v = i + k + len / 2;

                        float tReal = curReal * real[v] - curImag * imag[v];
                        float tImag = curReal * imag[v] + curImag * real[v];

                        real[v] = real[u] - tReal;
                        imag[v] = imag[u] - tImag;
                        real[u] += tReal;
                        imag[u] += tImag;

                        float newCurReal = curReal * wReal - curImag * wImag;
                        float newCurImag = curReal * wImag + curImag * wReal;
                        curReal = newCurReal;
                        curImag = newCurImag;
                    }
                }
            }
        }
    }
}
