for v in 0.23.0 0.24.0 0.25.0 0.26.0 0.27.0 0.28.0 0.29.0 0.30.0 0.31.0 0.32.0 0.33.0 0.34.0 0.35.0; do
  sed -i '' "s/deadpool-sqlite = .*/r2d2_sqlite = \"$v\"/" Cargo.toml
  echo "Trying r2d2_sqlite $v"
  if cargo check 2>/dev/null; then
    echo "SUCCESS: $v"
    break
  fi
  sed -i '' "s/r2d2_sqlite = .*/deadpool-sqlite = \"0.12\"/" Cargo.toml
done
