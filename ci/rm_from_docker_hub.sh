#!/usr/bin/env bash

# SPDX-FileCopyrightText: 2019-2024 Michael Picht <mipi@fsfe.org>
#
# SPDX-License-Identifier: GPL-3.0-or-later

#
# Script to remove tags from a repository on docker hub.
# Parameters:
# - docker hub user name       ($1)
# - docker hub password        ($2)
# - docker hub repository name ($3)
# - list to tags               ($4, ...)
#
# Dependencies:
# - bash
# - curl
# - jq

access_token=$(curl -s -H "Content-Type: application/json" -X POST -d '{"username": "'${1}'", "password": "'${2}'"}' https://hub.docker.com/v2/users/login/ | jq -r .token)
repo_name=$3

shift 3

for tag in "$@"; do
    curl -s -H "Authorization: JWT ${access_token}" -X DELETE https://hub.docker.com/v2/repositories/${repo_name}/tags/${tag}/
done    
