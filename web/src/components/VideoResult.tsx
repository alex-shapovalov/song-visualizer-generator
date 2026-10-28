interface VideoResultProps {
  src: string;
  fileName: string;
}

export function VideoResult({ src, fileName }: VideoResultProps) {
  return (
    <div className="result">
      <video className="result-video" src={src} controls preload="metadata" />
      <a className="button" href={src} download={fileName}>
        Download MP4
      </a>
    </div>
  );
}
