#!/usr/bin/env python3
"""
PiSugar Charge-Discharge Cycle and SOC Table Generation Script

This script performs a full charge-discharge cycle, logs the discharge data in-memory,
and generates a state-of-charge (SOC) lookup table from it. The script finishes after
one cycle and automatically re-enables charging.
"""

import socket
import time
import os
import numpy as np
import json

# --- Configuration Parameters ---
HOST, PORT = "127.0.0.1", 8423
# Stop charging and start discharging after reaching this voltage
CHARGE_START_V = 4.19
# Stop discharging and start charging after reaching this voltage
DISCHARGE_END_V = 3.10
# Data logging interval (seconds)
INTERVAL_S = 5
# Number of points for the lookup table (excluding 100% and 0%)
NUM_SOC_POINTS = 15
# Output file for the lookup table
OUTPUT_JSON_FILE = 'battery_curve.json'

# --- PiSugar Control Functions ---


def send_cmd(cmd: str) -> str:
    """Sends a command, automatically filtering out lines starting with 'single' or 'double'"""
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        s.settimeout(2.0)
        s.connect((HOST, PORT))
        s.sendall((cmd + "\n").encode())
        s.shutdown(socket.SHUT_WR)
        buf = b""
        while True:
            try:
                chunk = s.recv(1024)
                if not chunk:
                    break
                buf += chunk
            except socket.timeout:
                break
    lines = [ln.strip() for ln in buf.decode().splitlines()]
    lines = [ln for ln in lines if not ln.startswith(("single", "double"))]
    if not lines:
        raise RuntimeError("All response lines were filtered, no valid data.")
    return lines[-1]


def get_battery_v() -> float:
    """Get the current battery voltage"""
    raw = send_cmd("get battery_v")
    last = raw.splitlines()[-1].strip()
    if last.startswith("battery_v:"):
        return float(last.split(":", 1)[1])
    raise RuntimeError(f"Unexpected response: {raw!r}")


def set_charging(enable: bool):
    """Enable or disable charging"""
    state = "true" if enable else "false"
    resp = send_cmd(f"set_allow_charging {state}")
    if "done" not in resp.lower():
        raise RuntimeError(f"set_allow_charging failed: {resp}")

# --- Cycle and Logging Logic ---


def run_cycle_and_log() -> list:
    """Executes one charge-discharge cycle and logs discharge data in memory, returns the data list"""
    # Charging Phase
    set_charging(True)
    print("Charging enabled, waiting for voltage ≥ %.2f V..." % CHARGE_START_V)
    while True:
        v = get_battery_v()
        print(f"Charging, voltage={v:.3f} V")
        if v >= CHARGE_START_V:
            print("Charging complete, starting discharge.")
            break
        time.sleep(INTERVAL_S)

    # Discharging Phase
    set_charging(False)
    print("Battery full (%.2f V), turning off charging, starting to log discharge data..." %
          get_battery_v())

    discharge_data = []
    start_time = time.monotonic()

    while True:
        v = get_battery_v()
        elapsed_s = time.monotonic() - start_time

        # Log data to the in-memory list
        discharge_data.append([elapsed_s, v])

        print(f"Time elapsed={elapsed_s:.1f} s, voltage={v:.3f} V")

        if v <= DISCHARGE_END_V:
            print("Discharge complete, preparing to generate the lookup table.")
            break

        time.sleep(INTERVAL_S)

    print("-" * 30)
    return discharge_data

# --- Lookup Table Generation and Saving Logic ---


def generate_soc_table(data_list: list, num_points: int) -> list | None:
    """Generates a concise SOC-Voltage lookup table from a list of data points."""
    if not data_list:
        print("Error: No data points to generate lookup table.")
        return None

    # Filter out invalid voltage readings
    valid_data = [d for d in data_list if d[1] > 0]
    if not valid_data:
        print("Error: All voltage readings were zero or invalid.")
        return None

    # Use NumPy arrays for easier slicing and calculations
    data_array = np.array(valid_data)
    elapsed_seconds = data_array[:, 0]
    voltages = data_array[:, 1]

    total_time = elapsed_seconds[-1]
    soc_percentages = 100 * (1 - elapsed_seconds / total_time)

    soc_table = [[round(voltages[0], 2), 100],
                 [round(voltages[-1], 2), 0]]

    target_socs = np.linspace(100, 0, num_points + 2)[1:-1]

    for target_soc in target_socs:
        # Find the index of the closest SOC percentage
        idx = np.argmin(np.abs(soc_percentages - target_soc))
        voltage = round(voltages[idx], 2)
        soc = int(round(soc_percentages[idx]))
        soc_table.append([voltage, soc])

    unique_points = {}
    for voltage, soc in soc_table:
        if voltage not in unique_points or unique_points[voltage] < soc:
            unique_points[voltage] = soc

    final_table = sorted(
        [[v, s] for v, s in unique_points.items()], key=lambda x: x[0], reverse=True)

    # Ensure start and end points are exactly 100% and 0%
    if final_table[0][1] != 100:
        final_table[0][1] = 100
    if final_table[-1][1] != 0:
        final_table[-1][1] = 0

    return final_table


def save_table_to_json(table: list, file_path: str):
    """Saves the lookup table to a JSON file in the format "battery_curve": [...]"""
    try:
        data = {"battery_curve": table}
        with open(file_path, 'w') as f:
            json.dump(data, f)
        print(
            f"Lookup table successfully saved to JSON file: {os.path.abspath(file_path)}")
    except IOError as e:
        print(f"Error while saving file: {e}")

# --- Main function to control the process flow ---


def main():
    """Main function to control the process flow"""
    discharge_data = None
    try:
        # Execute a single charge-discharge cycle and get the data in a list
        discharge_data = run_cycle_and_log()

        # Use the in-memory data to create the lookup table
        print("\nGenerating a SOC-Voltage lookup table from discharge curve data...")

        soc_lookup_table = generate_soc_table(
            discharge_data, num_points=NUM_SOC_POINTS)

        if soc_lookup_table:
            save_table_to_json(soc_lookup_table, OUTPUT_JSON_FILE)
            print("\nGenerated SOC-Voltage Lookup Table:")
            print(soc_lookup_table)
            print("-" * 50)
        else:
            print("Unable to generate lookup table. The script will now terminate.")

    except KeyboardInterrupt:
        print("\nUser interruption. Re-enabling charging...")
    except Exception as e:
        print(f"\nAn error occurred: {e}")
    finally:
        set_charging(True)
        print("Script finished. Charging has been re-enabled.")


if __name__ == "__main__":
    main()
