#!/usr/bin/env python3

import sys
import os
import subprocess
import tempfile
import scipy.io.wavfile as wavfile
from pyAudioAnalysis import ShortTermFeatures as aF
from pyAudioAnalysis import MidTermFeatures as mF
import numpy as np
import shutil

def extract_vocals(input_path, output_path):
    """Extract vocals from audio using Demucs."""
    try:
        # Create a temporary directory for Demucs output
        with tempfile.TemporaryDirectory() as temp_dir:
            print(f"Running Demucs on {input_path}...", file=sys.stderr)
            # Run Demucs to separate vocals
            subprocess.run([
                'demucs',
                '--two-stems=vocals',  # Split into vocals and accompaniment
                '-n', 'htdemucs',      # Use the htdemucs model
                '--mp3',               # Output as MP3 to save space
                '-o', temp_dir,        # Output directory
                input_path
            ], check=True, capture_output=True)
            
            # Get the vocals file path
            input_name = os.path.splitext(os.path.basename(input_path))[0]
            vocals_path = os.path.join(temp_dir, 'htdemucs', input_name, 'vocals.mp3')
            
            if not os.path.exists(vocals_path):
                raise RuntimeError("Demucs failed to extract vocals")
            
            print(f"Converting extracted vocals to WAV...", file=sys.stderr)
            # Convert vocals to WAV with normalization
            subprocess.run([
                'ffmpeg',
                '-v', 'error',
                '-i', vocals_path,
                '-af', 'loudnorm=I=-16:LRA=11:TP=-1.5,aresample=44100',
                '-ac', '1',
                '-acodec', 'pcm_s16le',
                '-ar', '44100',
                '-y',
                output_path
            ], check=True, capture_output=True)
            
    except subprocess.CalledProcessError as e:
        raise RuntimeError(f"Vocal extraction failed: {e.stderr.decode()}")

def convert_to_wav(input_path, output_path):
    """Convert input audio file to WAV format using ffmpeg."""
    try:
        print(f"Converting {input_path} to WAV...", file=sys.stderr)
        # Convert to WAV using ffmpeg with normalized audio and resampling
        subprocess.run([
            'ffmpeg',
            '-v', 'error',
            '-i', input_path,
            '-af', 'loudnorm=I=-16:LRA=11:TP=-1.5,aresample=44100',
            '-ac', '1',
            '-acodec', 'pcm_s16le',
            '-ar', '44100',
            '-y',
            output_path
        ], check=True, capture_output=True)
    except subprocess.CalledProcessError as e:
        raise RuntimeError(f"FFmpeg conversion failed: {e.stderr.decode()}")

def analyze_audio_chunk(x, Fs):
    """Analyze a chunk of audio and return its characteristics."""
    try:
        # Check if chunk contains any non-zero values
        if np.all(np.abs(x) < 1e-6):
            print(f"Warning: Chunk contains only silence", file=sys.stderr)
            return None

        print(f"Analyzing chunk of {len(x)} samples...", file=sys.stderr)
        # Extract short-term features
        F, _ = aF.feature_extraction(x, Fs, 0.050*Fs, 0.025*Fs)
        
        if F.shape[1] == 0:
            print(f"Warning: No features extracted from chunk", file=sys.stderr)
            return None
            
        # Get F0 values and filter out unrealistic frequencies
        f0_values = F[0,:]
        f0_values = f0_values[(f0_values > 50) & (f0_values < 300)]
        
        if len(f0_values) == 0:
            print(f"Warning: No valid fundamental frequencies found in chunk", file=sys.stderr)
            return None
            
        # Basic features
        median_f0 = np.median(f0_values)
        spectral_centroid = np.mean(F[3,:])
        spectral_rolloff = np.mean(F[4,:])
        zcr = np.mean(F[2,:])
        
        print(f"Chunk analysis results: F0={median_f0:.1f}Hz, centroid={spectral_centroid:.1f}, rolloff={spectral_rolloff:.1f}, zcr={zcr:.3f}", file=sys.stderr)
        
        return {
            'median_f0': median_f0,
            'spectral_centroid': spectral_centroid,
            'spectral_rolloff': spectral_rolloff,
            'zcr': zcr
        }
    except Exception as e:
        print(f"Warning: Error analyzing chunk: {str(e)}", file=sys.stderr)
        return None

def detect_gender(audio_path):
    try:
        print(f"\nAnalyzing audio file: {audio_path}", file=sys.stderr)
        
        # Create temporary files
        with tempfile.NamedTemporaryFile(suffix='.wav', delete=True) as vocals_wav, \
             tempfile.NamedTemporaryFile(suffix='.wav', delete=True) as temp_wav:
            
            print("\nStep 1: Extracting vocals from audio...", file=sys.stderr)
            try:
                # First try to extract vocals using Demucs
                extract_vocals(audio_path, vocals_wav.name)
                wav_path = vocals_wav.name
                print("Successfully extracted vocals", file=sys.stderr)
            except Exception as e:
                print(f"Warning: Could not extract vocals ({str(e)}), using original audio", file=sys.stderr)
                # If Demucs fails, fall back to using the original audio
                convert_to_wav(audio_path, temp_wav.name)
                wav_path = temp_wav.name
            
            print("\nStep 2: Loading and preprocessing audio...", file=sys.stderr)
            # Load audio file
            Fs, x = wavfile.read(wav_path)
            x = x.astype(float)
            
            print(f"Loaded audio: {len(x)} samples, {Fs}Hz", file=sys.stderr)
            
            # Check if audio is empty or contains only silence
            if len(x) == 0:
                raise RuntimeError("Audio file is empty")
            
            if np.all(np.abs(x) < 1e-6):
                raise RuntimeError("Audio file contains only silence")
            
            # Normalize audio
            max_amp = np.max(np.abs(x))
            if max_amp > 0:
                x = x / max_amp
            else:
                raise RuntimeError("Audio file has zero amplitude")
            
            print("\nStep 3: Splitting audio into chunks...", file=sys.stderr)
            # Split audio into 3-second chunks with 1-second overlap
            chunk_size = int(3 * Fs)
            hop_size = int(1 * Fs)
            
            chunks = []
            for i in range(0, len(x) - chunk_size + 1, hop_size):
                chunk = x[i:i + chunk_size]
                if len(chunk) == chunk_size:  # Only use complete chunks
                    chunks.append(chunk)
            
            # If audio is shorter than 3 seconds, use the entire file
            if not chunks and len(x) > 0:
                print("Audio shorter than 3 seconds, analyzing entire file", file=sys.stderr)
                chunks = [x]
            
            print(f"Created {len(chunks)} chunks for analysis", file=sys.stderr)
            
            print("\nStep 4: Analyzing chunks...", file=sys.stderr)
            # Analyze each chunk
            chunk_results = []
            for i, chunk in enumerate(chunks):
                print(f"\nAnalyzing chunk {i+1}/{len(chunks)}...", file=sys.stderr)
                result = analyze_audio_chunk(chunk, Fs)
                if result:
                    chunk_results.append(result)
            
            if not chunk_results:
                raise RuntimeError("Could not analyze any audio chunks")
            
            print(f"\nStep 5: Aggregating results from {len(chunk_results)} chunks...", file=sys.stderr)
            # Aggregate results from all chunks
            median_f0 = np.median([r['median_f0'] for r in chunk_results])
            spectral_centroid = np.mean([r['spectral_centroid'] for r in chunk_results])
            spectral_rolloff = np.mean([r['spectral_rolloff'] for r in chunk_results])
            zcr = np.mean([r['zcr'] for r in chunk_results])
            
            # Simple scoring system focusing on fundamental frequency
            gender_score = 0.0
            
            # Fundamental frequency scoring (primary factor)
            if 85 <= median_f0 <= 155:  # Clear male range
                gender_score += 2.0
            elif 155 < median_f0 <= 170:  # Probable male
                gender_score += 1.0
            elif 170 < median_f0 <= 255:  # Female range
                gender_score -= 1.0
            
            # Secondary characteristics
            if spectral_centroid < 1600:  # Male characteristic
                gender_score += 0.5
            
            if zcr < 0.1:  # Male characteristic
                gender_score += 0.5
            
            # Debug information
            print(f"\nFinal Analysis Results:", file=sys.stderr)
            print(f"Number of analyzed chunks: {len(chunk_results)}", file=sys.stderr)
            print(f"Median F0: {median_f0:.2f} Hz", file=sys.stderr)
            print(f"Spectral centroid: {spectral_centroid:.2f}", file=sys.stderr)
            print(f"Spectral rolloff: {spectral_rolloff:.2f}", file=sys.stderr)
            print(f"Zero crossing rate: {zcr:.4f}", file=sys.stderr)
            print(f"Final gender score: {gender_score:.2f}", file=sys.stderr)
            
            # Final decision
            is_male = gender_score > 0.0
            
            print("male" if is_male else "female")
            
    except Exception as e:
        print(f"error: {str(e)}", file=sys.stderr)
        sys.exit(1)

if __name__ == "__main__":
    if len(sys.argv) != 2:
        print("error: Audio file path required", file=sys.stderr)
        sys.exit(1)
    detect_gender(sys.argv[1]) 