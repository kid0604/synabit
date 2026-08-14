cp src-tauri/src/sync/coordinator.rs scratch/coordinator.rs
cp scratch/patch_coordinator_tests.py scratch/patch_coordinator_tests_copy.py
sed -i '' 's|"src/sync/coordinator.rs"|"scratch/coordinator.rs"|g' scratch/patch_coordinator_tests_copy.py

sed -i '' 's|"src-tauri/src/sync/coordinator.rs"|"scratch/coordinator.rs"|g' scratch/patch_coordinator.py
sed -i '' 's|"src-tauri/src/sync/coordinator.rs"|"scratch/coordinator.rs"|g' scratch/patch_coordinator_2.py
sed -i '' 's|"src-tauri/src/sync/coordinator.rs"|"scratch/coordinator.rs"|g' scratch/patch_coordinator_3.py

python3 scratch/patch_coordinator.py
python3 scratch/patch_coordinator_2.py
python3 scratch/patch_coordinator_3.py
python3 scratch/patch_coordinator_tests_copy.py

cp scratch/coordinator.rs src-tauri/src/sync/coordinator.rs
