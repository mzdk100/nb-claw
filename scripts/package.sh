#!/bin/bash
# nb-claw Linux Packaging Script
# Creates a .deb package for Debian/Ubuntu systems
# Supports multiple architectures: amd64, arm64, armhf

set -e

PACKAGE_NAME="nb-claw"
VERSION=$(grep '^version = ' Cargo.toml | sed 's/version = "\(.*\)"/\1/')

# Detect target architecture
detect_arch() {
    local target=$(rustc -vV | grep 'host:' | sed 's/host: //')
    case "$target" in
        x86_64-*-linux-*)
            echo "amd64"
            ;;
        aarch64-*-linux-*)
            echo "arm64"
            ;;
        arm-*-linux-*hf)
            echo "armhf"
            ;;
        *)
            echo "amd64"  # fallback
            ;;
    esac
}

ARCH=${1:-$(detect_arch)}
DEB_DIR="${PACKAGE_NAME}_${VERSION}_${ARCH}"
DIST_BIN="target/release/nb-claw"
INSTALL_DIR="/opt/${PACKAGE_NAME}"

echo "=== nb-claw Linux Packaging Script ==="
echo "Version: ${VERSION}"
echo "Architecture: ${ARCH}"

# Step 1: Build release binary
echo "[1/7] Building release binary..."
cargo build --release

# Step 2: Check if binary exists
if [ ! -f "$DIST_BIN" ]; then
    echo "Error: Binary not found at $DIST_BIN"
    exit 1
fi

# Step 3: Clean existing deb directory
echo "[2/7] Cleaning existing deb directory..."
rm -rf "$DEB_DIR"

# Step 4: Create deb package structure
echo "[3/7] Creating deb package structure..."
mkdir -p "${DEB_DIR}/DEBIAN"
mkdir -p "${DEB_DIR}${INSTALL_DIR}/bin"
mkdir -p "${DEB_DIR}/usr/share/doc/${PACKAGE_NAME}"
mkdir -p "${DEB_DIR}/usr/bin"

# Step 5: Copy binary and documentation
echo "[4/7] Copying binary and documentation..."
cp "$DIST_BIN" "${DEB_DIR}${INSTALL_DIR}/bin/"

# Copy documentation
cp README.md "${DEB_DIR}/usr/share/doc/${PACKAGE_NAME}/" 2>/dev/null || true
cp CHANGELOGS.md "${DEB_DIR}/usr/share/doc/${PACKAGE_NAME}/" 2>/dev/null || true
cp CONFIG_GUIDE.md "${DEB_DIR}/usr/share/doc/${PACKAGE_NAME}/" 2>/dev/null || true

# Create symlink in /usr/bin
ln -s "${INSTALL_DIR}/bin/nb-claw" "${DEB_DIR}/usr/bin/nb-claw"

# Step 6: Create control file
echo "[5/7] Creating control file..."
cat > "${DEB_DIR}/DEBIAN/control" << EOF
Package: ${PACKAGE_NAME}
Version: ${VERSION}
Section: utils
Priority: optional
Architecture: ${ARCH}
Maintainer: nb-claw <mzdk100@users.noreply.github.com>
Description: An AI assistant with autonomous planning and execution
 nb-claw is an AI assistant implemented in Rust that has autonomous
 planning and execution capabilities. It embeds a Python interpreter,
 supports shell commands, and features a revolutionary memory system.
Homepage: https://github.com/mzdk100/nb-claw
Depends: libc6 (>= 2.17), libgcc-s1 (>= 4.2), python3 (>= 3.10)
EOF

# Create postinst script (initialize config)
echo "[6/7] Creating postinst script..."
cat > "${DEB_DIR}/DEBIAN/postinst" << 'EOF'
#!/bin/bash
set -e

# Create config directory in user's home if it doesn't exist
if [ ! -d "/opt/nb-claw/config" ]; then
    mkdir -p /opt/nb-claw/config
fi

# Create data directory
if [ ! -d "/opt/nb-claw/data" ]; then
    mkdir -p /opt/nb-claw/data
fi

echo "nb-claw has been installed. Run 'nb-claw --init-config' to initialize configuration."
exit 0
EOF

chmod 755 "${DEB_DIR}/DEBIAN/postinst"

# Create prerm script
cat > "${DEB_DIR}/DEBIAN/prerm" << 'EOF'
#!/bin/bash
set -e
echo "Removing nb-claw..."
exit 0
EOF

chmod 755 "${DEB_DIR}/DEBIAN/prerm"

# Step 7: Build deb package
echo "[7/7] Building deb package..."
dpkg-deb --build --root-owner-group "$DEB_DIR"

# Cleanup
rm -rf "$DEB_DIR"

echo "=== Package created: ${DEB_DIR}.deb ==="
echo "Done!"
