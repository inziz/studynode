const REPO_SLUG = "inziz/studynode";

const FALLBACK_REPO = {
  stars: 0,
  forks: 0,
  language: "Rust",
  latest_release: "v0.6.0",
};

const FALLBACK_CONTRIBUTORS = [
  {
    login: "inziz",
    avatar_url: "https://avatars.githubusercontent.com/u/59830723?v=4",
    html_url: "https://github.com/inziz",
  },
  {
    login: "elsanti33",
    avatar_url: "https://avatars.githubusercontent.com/u/111271831?v=4",
    html_url: "https://github.com/elsanti33",
  },
];

function renderAvatarLinks(containerId, contributors) {
  const container = document.getElementById(containerId);
  if (!container) return;

  container.innerHTML = contributors
    .map(
      (c) => `
      <a
        class="avatar-link"
        href="${c.html_url}"
        target="_blank"
        rel="noreferrer"
        title="${c.login} on GitHub"
        aria-label="${c.login} on GitHub"
      >
        <img src="${c.avatar_url}" alt="${c.login}" loading="lazy" />
      </a>`
    )
    .join("");
}

function renderStats(stats) {
  const stars = document.getElementById("stat-stars");
  const forks = document.getElementById("stat-forks");
  const release = document.getElementById("stat-release");
  const language = document.getElementById("stat-language");

  if (stars) stars.textContent = String(stats.stars);
  if (forks) forks.textContent = String(stats.forks);
  if (release) release.textContent = stats.latest_release;
  if (language) language.textContent = stats.language;
}

async function hydrateFromGitHub() {
  const syncNote = document.getElementById("live-sync-note");
  try {
    const [repoRes, relRes, contribRes] = await Promise.all([
      fetch(`https://api.github.com/repos/${REPO_SLUG}`),
      fetch(`https://api.github.com/repos/${REPO_SLUG}/releases/latest`),
      fetch(`https://api.github.com/repos/${REPO_SLUG}/contributors`),
    ]);

    if (!repoRes.ok) return;

    const repo = await repoRes.json();
    const rel = relRes.ok ? await relRes.json() : null;
    const rawContribs = contribRes.ok ? await contribRes.json() : null;

    renderStats({
      stars: repo.stargazers_count ?? FALLBACK_REPO.stars,
      forks: repo.forks_count ?? FALLBACK_REPO.forks,
      language: repo.language || FALLBACK_REPO.language,
      latest_release: rel?.tag_name || FALLBACK_REPO.latest_release,
    });

    if (Array.isArray(rawContribs) && rawContribs.length > 0) {
      const list = rawContribs.map((item) => ({
        login: item.login,
        avatar_url: item.avatar_url,
        html_url: item.html_url,
      }));
      renderAvatarLinks("top-contributors", list);
      renderAvatarLinks("footer-contributors", list);
    }

    if (syncNote) {
      syncNote.textContent = `${REPO_SLUG} · live stats`;
    }
  } catch {
    // Keep seeded snapshot when offline or repository is private.
  }
}

function setupSegmentedNav() {
  const links = Array.from(document.querySelectorAll(".doc-segmented a"));
  links.forEach((link) => {
    link.addEventListener("click", () => {
      links.forEach((l) => l.classList.remove("is-active"));
      link.classList.add("is-active");
    });
  });
}

function setupCopyButtons() {
  document.querySelectorAll(".copy-btn").forEach((btn) => {
    btn.addEventListener("click", async () => {
      const targetId = btn.getAttribute("data-copy-target");
      const target = targetId ? document.getElementById(targetId) : null;
      if (!target) return;
      try {
        await navigator.clipboard.writeText(target.textContent || "");
        const prev = btn.textContent;
        btn.textContent = "Copied";
        setTimeout(() => {
          btn.textContent = prev;
        }, 1600);
      } catch {
        // Ignore clipboard errors on restricted contexts.
      }
    });
  });
}

renderStats(FALLBACK_REPO);
renderAvatarLinks("top-contributors", FALLBACK_CONTRIBUTORS);
renderAvatarLinks("footer-contributors", FALLBACK_CONTRIBUTORS);
setupSegmentedNav();
setupCopyButtons();
hydrateFromGitHub();
