namespace Yap.Core
{
    /// <summary>
    /// State machine for the application's recording/processing pipeline.
    /// Guards against overlapping operations.
    /// </summary>
    public enum AppState
    {
        /// <summary>No recording or processing in progress. Ready for input.</summary>
        Idle,

        /// <summary>Hold-to-record: actively recording while hotkey is held down.</summary>
        Recording,

        /// <summary>Hands-free recording: recording without holding the hotkey (double-tap or click activated).</summary>
        HandsFreeRecording,

        /// <summary>Hands-free recording is paused. Audio engine stays running but data is not written.</summary>
        HandsFreePaused,

        /// <summary>Recording stopped; audio is being transcribed and/or formatted.</summary>
        Processing
    }
}
