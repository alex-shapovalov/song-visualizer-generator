import { useState, type FormEvent } from 'react';
import type { VisualizerInput } from '../api';
import { FilePicker } from './FilePicker';

interface VisualizerFormProps {
  disabled: boolean;
  onSubmit: (input: VisualizerInput) => void;
}

export function VisualizerForm({ disabled, onSubmit }: VisualizerFormProps) {
  const [title, setTitle] = useState('');
  const [artist, setArtist] = useState('');
  const [art, setArt] = useState<File | null>(null);
  const [audio, setAudio] = useState<File | null>(null);

  const isComplete = title.trim() !== '' && artist.trim() !== '' && art !== null && audio !== null;

  function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (art === null || audio === null || !isComplete) {
      return;
    }
    onSubmit({ title: title.trim(), artist: artist.trim(), art, audio });
  }

  return (
    <form onSubmit={handleSubmit}>
      <fieldset className="form" disabled={disabled}>
        <label className="field">
          <span>Song name</span>
          <input value={title} onChange={(event) => setTitle(event.target.value)} placeholder="echo (let go)" />
        </label>

        <label className="field">
          <span>Artist name</span>
          <input value={artist} onChange={(event) => setArtist(event.target.value)} placeholder="page.dwn" />
        </label>

        <div className="pickers">
          <FilePicker
            label="Album art"
            hint="JPEG, PNG or WebP"
            accept="image/jpeg,image/png,image/webp"
            file={art}
            showPreview
            onChange={setArt}
          />
          <FilePicker
            label="Audio"
            hint="WAV or AIFF"
            accept=".wav,.aif,.aiff,audio/wav,audio/x-wav,audio/aiff,audio/x-aiff"
            file={audio}
            onChange={setAudio}
          />
        </div>

        <button type="submit" className="button" disabled={!isComplete}>
          Generate video
        </button>
      </fieldset>
    </form>
  );
}
