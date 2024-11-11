# SPDX-FileCopyrightText: 2019-2024 Michael Picht <mipi@fsfe.org>
#
# SPDX-License-Identifier: GPL-3.0-or-later

PROG=repman

# Set project VERSION to last tag name. If no tag exists, set it to v0.0.0
$(eval TAGS=$(shell git rev-list --tags))
ifdef TAGS
	VERSION=$(shell git describe --tags --abbrev=0)
else
	VERSION=v0.0.0	
endif

# Build executable and man page 
all: $(PROG).1 $(PROG)

.PHONY: all clean install lint release

# Executable
$(PROG):
	cargo build --release

# man page
$(PROG).1: ./doc/manpage.adoc
	@asciidoctor -b manpage -d manpage -o "$(PROG).8" ./doc/manpage.adoc

lint:
	reuse lint

install:
	@install -Dm755 target/release/$(PROG) $(DESTDIR)/usr/bin/$(PROG)
	@install -Dm755 $(PROG)-all $(DESTDIR)/usr/bin/$(PROG)-all
	@install -Dm644 "$(PROG).8" -t "$(DESTDIR)/usr/share/man/man8/"
	@install -Dm644 "cfg/$(PROG).conf" -t "$(DESTDIR)/etc/"

# remove build results
clean:
	@rm -f "$(PROG).8"

# (1) Adjust version in Cargo.toml and in man documentation to RELEASE, commit
#     and push changes
# (2) Create an annotated tag with name RELEASE
release:
	@if [ -z $(RELEASE) ]; then \
		echo "no new release submitted"; \
		exit 1; \
	fi
	@VER_NEW=$(RELEASE); \
	VER_NEW=$${VER_NEW#v}; \
	VER_OLD=`sed -n "s/^version *= \"*\(.*\)\"/\1/p" ./Cargo.toml`; \
	if ! [ $$((`vercmp $${VER_OLD} $${VER_NEW}`)) -lt 0 ]; then \
		echo "new version is not greater than old version"; \
		exit 1; \
	fi; \
	sed -i -e "s/^version.*/version = \"$${VER_NEW#v}\"/" ./Cargo.toml; \
	sed -i -e "/Michael Picht/{n;s/^.*/Version $${VER_NEW#v}/}" ./doc/manpage.adoc
	@git commit -a -s -m "release $(RELEASE)"
	@git push
	@git tag -a $(RELEASE) -m "release $(RELEASE)"
	@git push origin $(RELEASE)
