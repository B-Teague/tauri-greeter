#!/bin/bash
# CSSDM Installation Test Script
# Verifies that CSSDM is properly installed and configured

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Test counters
PASSED=0
FAILED=0
WARNED=0

# Helper functions
pass() {
    echo -e "${GREEN}✓${NC} $1"
    ((PASSED++))
}

fail() {
    echo -e "${RED}✗${NC} $1"
    ((FAILED++))
}

warn() {
    echo -e "${YELLOW}⚠${NC} $1"
    ((WARNED++))
}

info() {
    echo -e "${BLUE}ℹ${NC} $1"
}

section() {
    echo ""
    echo -e "${BLUE}=== $1 ===${NC}"
}

# Main test suite
section "CSSDM Installation Tests"

# Test 1: Binary exists
section "Binary"
if [ -f /usr/local/bin/cssdm ]; then
    pass "Binary found at /usr/local/bin/cssdm"
    SIZE=$(du -h /usr/local/bin/cssdm | cut -f1)
    info "Binary size: $SIZE"
else
    fail "Binary not found at /usr/local/bin/cssdm"
fi

# Test 2: Binary is executable
if [ -x /usr/local/bin/cssdm ]; then
    pass "Binary is executable"
else
    fail "Binary is not executable"
fi

# Test 3: Theme directory exists
section "Themes"
if [ -d /usr/share/cssdm/themes ]; then
    pass "Theme directory exists at /usr/share/cssdm/themes"
else
    fail "Theme directory not found"
fi

# Test 4: Default themes installed
if [ -d /usr/share/cssdm/themes/light ]; then
    pass "Light theme installed"
else
    warn "Light theme not installed"
fi

if [ -d /usr/share/cssdm/themes/high-contrast ]; then
    pass "High-contrast theme installed"
else
    warn "High-contrast theme not installed"
fi

# Test 5: Theme metadata
section "Theme Metadata"
for theme_dir in /usr/share/cssdm/themes/*/; do
    theme_name=$(basename "$theme_dir")
    if [ -f "$theme_dir/theme.json" ]; then
        if jq . "$theme_dir/theme.json" >/dev/null 2>&1; then
            pass "Theme '$theme_name' has valid JSON"
        else
            fail "Theme '$theme_name' has invalid JSON"
        fi
    else
        warn "Theme '$theme_name' missing theme.json"
    fi

    if [ -f "$theme_dir/theme.css" ]; then
        pass "Theme '$theme_name' has CSS file"
    else
        warn "Theme '$theme_name' missing theme.css"
    fi
done

# Test 6: System files
section "System Integration"
if [ -f /etc/systemd/system/cssdm.service ]; then
    pass "systemd service installed"

    if systemctl is-enabled cssdm.service >/dev/null 2>&1; then
        pass "Service is enabled"
    else
        warn "Service is not enabled"
    fi
else
    warn "systemd service not installed"
fi

# Test 7: Display manager alternatives
section "Display Manager Configuration"
if update-alternatives --query x-session-manager >/dev/null 2>&1; then
    if update-alternatives --query x-session-manager | grep -q cssdm; then
        pass "CSSDM registered as display manager"
    else
        warn "CSSDM not registered as display manager"
    fi
else
    warn "update-alternatives not available (not Debian-based?)"
fi

# Test 8: PAM configuration
section "PAM Configuration"
if [ -f /etc/pam.d/login ]; then
    pass "PAM login config exists"

    # Check for basic PAM rules
    if grep -q "^auth" /etc/pam.d/login; then
        pass "PAM auth rule configured"
    else
        warn "No auth rule in PAM config"
    fi

    if grep -q "^account" /etc/pam.d/login; then
        pass "PAM account rule configured"
    else
        warn "No account rule in PAM config"
    fi

    if grep -q "^session" /etc/pam.d/login; then
        pass "PAM session rule configured"
    else
        warn "No session rule in PAM config"
    fi
else
    warn "PAM login config not found"
fi

# Test 9: Desktop sessions
section "Desktop Sessions"
if [ -d /usr/share/xsessions ]; then
    x11_count=$(ls /usr/share/xsessions/*.desktop 2>/dev/null | wc -l)
    if [ $x11_count -gt 0 ]; then
        pass "Found $x11_count X11 session(s)"
    else
        warn "No X11 sessions found"
    fi
else
    warn "X11 sessions directory not found"
fi

if [ -d /usr/share/wayland-sessions ]; then
    wayland_count=$(ls /usr/share/wayland-sessions/*.desktop 2>/dev/null | wc -l)
    if [ $wayland_count -gt 0 ]; then
        pass "Found $wayland_count Wayland session(s)"
    else
        warn "No Wayland sessions found"
    fi
else
    warn "Wayland sessions directory not found"
fi

# Test 10: User home directories
section "User Configuration"
if [ -r /etc/passwd ]; then
    pass "Can read /etc/passwd"
    user_count=$(wc -l < /etc/passwd)
    info "Total users: $user_count"
else
    fail "Cannot read /etc/passwd"
fi

# Test 11: Firewall/Security
section "Security"
# Check if CSSDM binary has proper permissions
perms=$(stat -c '%a' /usr/local/bin/cssdm 2>/dev/null)
if [ "$perms" = "755" ]; then
    pass "Binary has correct permissions (755)"
elif [ "$perms" = "750" ]; then
    warn "Binary permissions are 750 (consider 755 for broader access)"
else
    warn "Binary has unexpected permissions ($perms)"
fi

# Test 12: Disk space
section "System Resources"
free_space=$(df /usr/share/cssdm 2>/dev/null | awk 'NR==2 {print $4}')
if [ -n "$free_space" ] && [ "$free_space" -gt 1000 ]; then
    pass "Sufficient free space available"
else
    warn "Low disk space in /usr/share"
fi

# Summary
section "Test Summary"
total=$((PASSED + FAILED + WARNED))
echo ""
echo -e "Total tests: $total"
echo -e "  ${GREEN}Passed: $PASSED${NC}"
if [ $FAILED -gt 0 ]; then
    echo -e "  ${RED}Failed: $FAILED${NC}"
fi
if [ $WARNED -gt 0 ]; then
    echo -e "  ${YELLOW}Warnings: $WARNED${NC}"
fi

# Final verdict
echo ""
if [ $FAILED -eq 0 ]; then
    if [ $WARNED -eq 0 ]; then
        echo -e "${GREEN}✓ All tests passed!${NC}"
        echo ""
        echo "CSSDM is ready to use. Next steps:"
        echo "  1. Test login: sudo systemctl start cssdm"
        echo "  2. Or reboot to use as default display manager: sudo reboot"
    else
        echo -e "${YELLOW}✓ Tests passed with warnings${NC}"
        echo ""
        echo "Review warnings above before deploying to production."
    fi
    exit 0
else
    echo -e "${RED}✗ Some tests failed${NC}"
    echo ""
    echo "See INSTALL.md troubleshooting section for help."
    exit 1
fi
