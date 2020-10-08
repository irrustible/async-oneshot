.PHONY: bundle clean-bundle make-bundle environment-data

DATA = target/bench/data
CSV_DIRS = $(shell find target/criterion/ -type d -name new)
CSVS = $(foreach d,$(subst /,___,$(subst /new,,$(subst target/criterion/,,$(CSV_DIRS)))),$(DATA)/$(d).csv)

bundle: clean-bundle make-bundle
	echo raw $(CSV_DIRS)
	echo target $(CSVS)

clean-bundle:
	rm -Rf target/bench
	mkdir -p $(DATA)

prepare-bundle: environment-data $(CSVS)

make-bundle: prepare-bundle
	tar zcvf target/bench/bundle.tar.gz target/bench/data/*

environment-data:
	cp /proc/cpuinfo target/bench/data/cpuinfo || true
	uname -a > target/bench/data/uname
	git branch --show-current > target/bench/data/branch

target/bench/data/%.csv:
	echo $(subst .csv,/new/raw.csv,$(subst ___,/,$(subst target/bench/data/,target/criterion/,$@)))
	cp $(subst .csv,/new/raw.csv,$(subst ___,/,$(subst target/bench/data/,target/criterion/,$@))) $@

