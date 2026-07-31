export type UpdateState = "idle" | "checking" | "current" | "available" | "error";

export type GitHubReleaseAsset = {
  name: string;
  browser_download_url: string;
};

export type GitHubRelease = {
  tag_name: string;
  html_url: string;
  assets: GitHubReleaseAsset[];
};

// Set this to null before release builds. Keeping it as an object forces the update badge for UI review.
export const MOCK_LATEST_RELEASE: GitHubRelease | null = null;
// export const MOCK_LATEST_RELEASE: GitHubRelease | null = {
//   tag_name: "v0.2.0",
//   html_url: "https://github.com/Rinzller/DCS-Mission-Composer/releases/tag/v0.2.0",
//   assets: [
//     {
//       name: "DCS-Mission-Composer_0.2.0_windows_x86_64-setup.exe",
//       browser_download_url:
//         "https://github.com/Rinzller/DCS-Mission-Composer/releases/download/v0.2.0/DCS-Mission-Composer_0.2.0_windows_x86_64-setup.exe",
//     },
//   ],
// };

export const githubApiUrl = (repoUrl: string) => {
  const match = repoUrl.match(/^https:\/\/github\.com\/([^/]+)\/([^/]+)\/?$/);
  return match ? `https://api.github.com/repos/${match[1]}/${match[2]}/releases/latest` : "";
};

export const normalizeVersion = (version: string) => version.replace(/^v/i, "").split(/[+-]/)[0];

export const isNewerVersion = (latest: string, current: string) => {
  const latestParts = normalizeVersion(latest)
    .split(".")
    .map((part) => Number.parseInt(part, 10) || 0);
  const currentParts = normalizeVersion(current)
    .split(".")
    .map((part) => Number.parseInt(part, 10) || 0);
  const partCount = Math.max(latestParts.length, currentParts.length);

  for (let index = 0; index < partCount; index += 1) {
    const latestPart = latestParts[index] ?? 0;
    const currentPart = currentParts[index] ?? 0;

    if (latestPart > currentPart) {
      return true;
    }

    if (latestPart < currentPart) {
      return false;
    }
  }

  return false;
};

export const updateStateForRelease = (currentVersion: string, release: GitHubRelease): UpdateState =>
  isNewerVersion(release.tag_name, currentVersion) ? "available" : "current";

export const installerDownloadUrl = (release: GitHubRelease) => {
  const installer = release.assets.find((asset) => asset.name.toLocaleLowerCase().endsWith("-setup.exe"));
  return installer?.browser_download_url ?? release.html_url;
};
