#!/bin/bash
# Get the number of days from environment variable
days_to_keep=${DAYS_TO_KEEP:-1}

# Find files older than $days_to_keep days
find /app/.cache -type f -mtime +$days_to_keep -delete