// Why a resume of a session did not happen, under the session's header.
// When the session's folder is gone, the app offers to choose another one
// to resume it in.

export function ResumeProblem({
  message,
  onChooseFolder,
  busy = false,
}: {
  message: string;
  /** offered when the folder is gone and a folder can be chosen here */
  onChooseFolder?: () => void;
  busy?: boolean;
}) {
  return (
    <div className="notice notice-danger resume-problem" role="alert">
      <span>{message}</span>
      {onChooseFolder ? (
        <button type="button" className="chrome-button" disabled={busy} onClick={onChooseFolder}>
          Choose another folder…
        </button>
      ) : null}
    </div>
  );
}
