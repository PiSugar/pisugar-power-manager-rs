# systemd service

Commands of controlling pisugar-server systemd service

    # reload daemon
    sudo systemctl daemon-reload

    # check status
    sudo systemctl status pisugar-server

    # start service
    sudo systemctl start pisugar-server

    # stop service
    sudo systemctl stop pisugar-server

    # disable service
    sudo systemctl disable pisugar-server

    # enable service
    sudo systemctl enable pisugar-server

(pisugar-poweroff run once just before linux poweroff)

Journals

    journalctl -u pisugar-server