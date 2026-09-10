// CSSDM Login Manager - Tauri Integration

const { invoke } = window.__TAURI__.core;

// DOM Elements
const loginForm = document.getElementById("login-form");
const usernameSelect = document.getElementById("username");
const passwordInput = document.getElementById("password");
const sessionSelect = document.getElementById("session");
const errorMsg = document.getElementById("error-msg");
const loginBtn = document.getElementById("login-btn");
const powerMenuBtn = document.getElementById("power-menu-btn");
const powerMenu = document.getElementById("power-menu");
const cancelPowerBtn = document.getElementById("cancel-power-btn");
const loadingOverlay = document.getElementById("loading-overlay");
const powerActions = document.querySelectorAll(".power-action:not(.cancel)");
const themeToggleBtn = document.getElementById("theme-toggle-btn");
const themeMenu = document.getElementById("theme-menu");
const themeList = document.getElementById("theme-list");

// Current theme CSS element (injected dynamically)
let currentThemeStyle = null;

// Initialize on page load
document.addEventListener("DOMContentLoaded", () => {
  // Detect system theme preference
  detectSystemTheme();

  // Load all data in parallel
  Promise.all([loadUsers(), loadSessions(), loadThemes()])
    .then(() => {
      setupEventListeners();
      // Auto-focus username field
      usernameSelect.focus();
      console.log("CSSDM ready");
    })
    .catch((err) => {
      showError(`Failed to initialize: ${err}`);
      console.error("Initialization error:", err);
    });
});

// Detect system theme preference (light/dark)
function detectSystemTheme() {
  const savedTheme = localStorage.getItem("cssdm-theme");
  if (savedTheme && savedTheme !== "default") {
    // User has a saved preference, will be loaded later
    return;
  }

  // Check system preference
  if (window.matchMedia && window.matchMedia("(prefers-color-scheme: dark)").matches) {
    // System prefers dark - use default (already dark)
    // No action needed
  } else {
    // System prefers light - switch to light theme if available
    // This will happen after themes are loaded
  }
}

// Load users from backend
async function loadUsers() {
  try {
    const users = await invoke("get_available_users");
    usernameSelect.innerHTML = '<option value="">Select user...</option>';
    users.forEach((user) => {
      const option = document.createElement("option");
      option.value = user.username;
      option.textContent = `${user.full_name} (${user.username})`;
      usernameSelect.appendChild(option);
    });

    if (users.length > 0) {
      usernameSelect.value = users[0].username;
    }
  } catch (err) {
    showError(`Failed to load users: ${err}`);
  }
}

// Load sessions from backend
async function loadSessions() {
  try {
    const sessions = await invoke("get_available_sessions");
    sessionSelect.innerHTML = '<option value="">Select session...</option>';
    sessions.forEach((session) => {
      const option = document.createElement("option");
      option.value = session.name;
      const icon = session.is_wayland ? "🌊" : "🖥️";
      option.textContent = `${icon} ${session.name}`;
      sessionSelect.appendChild(option);
    });

    if (sessions.length > 0) {
      // Default to first Wayland, fallback to first session
      const waylandSession = sessions.find((s) => s.is_wayland);
      sessionSelect.value = waylandSession ? waylandSession.name : sessions[0].name;
    }
  } catch (err) {
    showError(`Failed to load sessions: ${err}`);
  }
}

// Load themes from backend
async function loadThemes() {
  try {
    const themes = await invoke("get_available_themes");
    themeList.innerHTML = "";
    themes.forEach((theme) => {
      const btn = document.createElement("button");
      btn.type = "button";
      btn.className = "theme-option";
      btn.textContent = theme.name;
      btn.dataset.themeId = theme.id;
      if (theme.id === "default") {
        btn.classList.add("active");
      }
      btn.addEventListener("click", (e) => {
        e.preventDefault();
        selectTheme(theme.id);
      });
      themeList.appendChild(btn);
    });
  } catch (err) {
    console.error(`Failed to load themes: ${err}`);
  }
}

// Select and apply a theme
async function selectTheme(themeId) {
  try {
    const css = await invoke("load_selected_theme", { themeId });

    // Remove existing dynamic theme style if any
    if (currentThemeStyle) {
      currentThemeStyle.remove();
    }

    // Create and inject new style
    currentThemeStyle = document.createElement("style");
    currentThemeStyle.id = `theme-${themeId}`;
    currentThemeStyle.textContent = css;
    document.head.appendChild(currentThemeStyle);

    // Update active indicator
    document.querySelectorAll(".theme-option").forEach((btn) => {
      btn.classList.remove("active");
      if (btn.dataset.themeId === themeId) {
        btn.classList.add("active");
      }
    });

    // Save preference to localStorage
    localStorage.setItem("cssdm-theme", themeId);

    // Close theme menu
    themeMenu.style.display = "none";
  } catch (err) {
    console.error(`Failed to load theme: ${err}`);
  }
}

// Setup event listeners
function setupEventListeners() {
  // Form submission
  loginForm.addEventListener("submit", (e) => {
    e.preventDefault();
    handleLogin();
  });

  // Theme menu toggle
  themeToggleBtn.addEventListener("click", (e) => {
    e.preventDefault();
    themeMenu.style.display =
      themeMenu.style.display === "none" ? "flex" : "none";
  });

  // Close theme menu when clicking outside
  document.addEventListener("click", (e) => {
    if (
      !e.target.closest(".theme-selector-container") &&
      themeMenu.style.display !== "none"
    ) {
      themeMenu.style.display = "none";
    }
  });

  // Power menu toggle
  powerMenuBtn.addEventListener("click", (e) => {
    e.preventDefault();
    powerMenu.style.display =
      powerMenu.style.display === "none" ? "flex" : "none";
  });

  // Cancel power menu
  cancelPowerBtn.addEventListener("click", () => {
    powerMenu.style.display = "none";
  });

  // Power actions
  powerActions.forEach((btn) => {
    btn.addEventListener("click", (e) => {
      e.preventDefault();
      const action = btn.dataset.action;
      handlePowerAction(action);
    });
  });

  // Hide error when user interacts with password field
  passwordInput.addEventListener("focus", () => {
    errorMsg.style.display = "none";
  });

  // Keyboard navigation improvements
  // Enter key in username/session selects → move to next field
  usernameSelect.addEventListener("keydown", (e) => {
    if (e.key === "Enter") {
      e.preventDefault();
      passwordInput.focus();
    }
  });

  sessionSelect.addEventListener("keydown", (e) => {
    if (e.key === "Enter") {
      e.preventDefault();
      loginBtn.click();
    }
  });

  // Escape key closes power/theme menus
  document.addEventListener("keydown", (e) => {
    if (e.key === "Escape") {
      if (powerMenu.style.display !== "none") {
        powerMenu.style.display = "none";
      }
      if (themeMenu.style.display !== "none") {
        themeMenu.style.display = "none";
      }
    }
  });

  // Load saved theme preference on startup
  const savedTheme = localStorage.getItem("cssdm-theme");
  if (savedTheme && savedTheme !== "default") {
    selectTheme(savedTheme);
  }
}

// Handle login
async function handleLogin() {
  const username = usernameSelect.value;
  const password = passwordInput.value;
  const session = sessionSelect.value;

  // Validate form
  if (!username) {
    showError("Please select a user");
    usernameSelect.focus();
    return;
  }
  if (!password) {
    showError("Please enter your password");
    passwordInput.focus();
    return;
  }
  if (!session) {
    showError("Please select a session");
    sessionSelect.focus();
    return;
  }

  // Show loading overlay and disable interactions
  loadingOverlay.style.display = "flex";
  loginBtn.disabled = true;
  powerMenuBtn.disabled = true;
  usernameSelect.disabled = true;
  passwordInput.disabled = true;
  sessionSelect.disabled = true;
  errorMsg.style.display = "none";

  try {
    // Authenticate user
    const authResult = await invoke("authenticate_user", {
      username,
      password,
    });

    if (!authResult.success) {
      // Authentication failed - show error and reset form
      const errorText = authResult.error || "Authentication failed";
      showError(errorText);
      resetLoginForm();
      loadingOverlay.style.display = "none";
      return;
    }

    // Set session for next login
    try {
      await invoke("set_session", { sessionName: session });
    } catch (sessionErr) {
      console.warn(`Failed to set session (non-critical): ${sessionErr}`);
      // Don't fail the login if session setting fails
    }

    // Login successful - update message
    const loadingText = loadingOverlay.querySelector("p");
    if (loadingText) {
      loadingText.textContent = `Welcome, ${username}! Starting session...`;
    }

    // In a real implementation, this would launch the session
    // For now, just log the success
    console.log(`User ${username} authenticated successfully with session ${session}`);

    // Keep loading overlay visible while session starts
    // The actual session will replace this window
  } catch (err) {
    showError(`Login error: ${err.toString()}`);
    resetLoginForm();
    loadingOverlay.style.display = "none";
  }
}

// Reset login form to initial state
function resetLoginForm() {
  loginBtn.disabled = false;
  powerMenuBtn.disabled = false;
  usernameSelect.disabled = false;
  passwordInput.disabled = false;
  sessionSelect.disabled = false;
  passwordInput.value = "";
  passwordInput.focus();
}

// Handle power actions
async function handlePowerAction(action) {
  const commandMap = {
    shutdown: "power_shutdown",
    reboot: "power_reboot",
    suspend: "power_suspend",
  };

  const command = commandMap[action];
  if (!command) return;

  // Show confirmation/loading
  powerMenu.style.display = "none";
  loadingOverlay.style.display = "flex";
  const loadingText = loadingOverlay.querySelector("p");
  const actionNames = {
    shutdown: "Shutting down",
    reboot: "Rebooting",
    suspend: "Suspending",
  };

  if (loadingText) {
    loadingText.textContent = `${actionNames[action]}...`;
  }

  try {
    await invoke(command);
    // If successful, the system will power off/reboot/suspend
    // This code may not execute because the system will shut down
  } catch (err) {
    const errorMsg = err.toString ? err.toString() : err;
    showError(`${actionNames[action]} failed: ${errorMsg}`);
    loadingOverlay.style.display = "none";
    console.error(`Power action '${action}' failed:`, err);
  }
}

// Show error message
function showError(message) {
  errorMsg.textContent = message;
  errorMsg.style.display = "block";
}
