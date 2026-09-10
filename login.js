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

// Initialize on page load
document.addEventListener("DOMContentLoaded", () => {
  loadUsers();
  loadSessions();
  setupEventListeners();
});

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

// Setup event listeners
function setupEventListeners() {
  // Form submission
  loginForm.addEventListener("submit", (e) => {
    e.preventDefault();
    handleLogin();
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
}

// Handle login
async function handleLogin() {
  const username = usernameSelect.value;
  const password = passwordInput.value;
  const session = sessionSelect.value;

  if (!username || !password || !session) {
    showError("Please fill in all fields");
    return;
  }

  // Show loading overlay
  loadingOverlay.style.display = "flex";
  loginBtn.disabled = true;
  powerMenuBtn.disabled = true;

  try {
    // Authenticate user
    const authResult = await invoke("authenticate_user", {
      username,
      password,
    });

    if (!authResult.success) {
      showError(authResult.error || "Authentication failed");
      loadingOverlay.style.display = "none";
      loginBtn.disabled = false;
      powerMenuBtn.disabled = false;
      passwordInput.value = "";
      passwordInput.focus();
      return;
    }

    // Set session
    await invoke("set_session", { sessionName: session });

    // Launch session (placeholder for future implementation)
    // For now, just show success message
    setTimeout(() => {
      console.log(`User ${username} logged in with session ${session}`);
      // In a real implementation, this would launch the session
      // and the window would be replaced by the session
    }, 1000);
  } catch (err) {
    showError(`Login error: ${err}`);
    loadingOverlay.style.display = "none";
    loginBtn.disabled = false;
    powerMenuBtn.disabled = false;
  }
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

  try {
    powerMenu.style.display = "none";
    loadingOverlay.style.display = "flex";
    await invoke(command);
    // If successful, the system will power off/reboot/suspend
    // This code may not execute
  } catch (err) {
    showError(`Power action failed: ${err}`);
    loadingOverlay.style.display = "none";
  }
}

// Show error message
function showError(message) {
  errorMsg.textContent = message;
  errorMsg.style.display = "block";
}
