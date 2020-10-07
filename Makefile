.PHONY: bundle clean-bundle make-bundle environment-data

DATA = target/bench/data
RAW_CSVS = $(wildcard target/criterion/*/new/raw.csv)
CSVS = $(subst /new/raw,,$(subst target/criterion/,$(DATA)/,$(RAW_CSVS)))

bundle: clean-bundle make-bundle

clean-bundle:
	rm -Rf target/bench
	mkdir -p $(DATA)

prepare-bundle: environment-data $(CSVS)

make-bundle: prepare-bundle
	tar zcvf target/bench/bundle.tar.gz target/bench/data/*

environment-data:
	cp /proc/cpuinfo target/bench/data/cpuinfo
	uname -a > target/bench/data/uname
	git branch --show-current > target/bench/data/branch

target/bench/data/%.csv: target/criterion/%/new/raw.csv
	cp $< $@
