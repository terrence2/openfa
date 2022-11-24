# This file is part of OpenFA.
#
# OpenFA is free software: you can redistribute it and/or modify
# it under the terms of the GNU General Public License as published by
# the Free Software Foundation, either version 3 of the License, or
# (at your option) any later version.
#
# OpenFA is distributed in the hope that it will be useful,
# but WITHOUT ANY WARRANTY; without even the implied warranty of
# MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
# GNU General Public License for more details.
#
# You should have received a copy of the GNU General Public License
# along with OpenFA.  If not, see <http://www.gnu.org/licenses/>.

V := 0.1.0
TMP := ofa-$(V)
PROJECT_ID := 18970786
OFA_BUCKET_URL := https://openfa.s3.us-west-1.amazonaws.com
PKG_OFA_LINUX_X64 := openfa-alpha-$(V)-linux-x86_64.tar.bz2
PKG_TOOLS_LINUX_X64 := ofa-tools-alpha-$(V)-linux-x86_64.tar.bz2
PKG_OFA_WIN_X64 := openfa-alpha-$(V)-win-x86_64.zip
PKG_TOOLS_WIN_X64 := ofa-tools-alpha-$(V)-win-x86_64.zip

clean:
	rm -rf $(TMP) \
		openfa-win-x86_64.zip \
		ofa-tools-win-x86_64.zip \
		openfa-linux-x86_64.tar.bz2 \
		ofa-tools-linux-x86_64.tar.bz2


website:
	pushd site && zola build && popd
	rsync -rav --stats --progress ./site/public/ root@openfa.home.arpa:/var/www/openfa.org/


target/x86_64-pc-windows-gnu/release/openfa.exe:
	cargo build --target x86_64-pc-windows-gnu --workspace --all-targets --release

openfa-win-x86_64.zip: target/x86_64-pc-windows-gnu/release/openfa.exe
	mkdir $(TMP)
	cp target/x86_64-pc-windows-gnu/release/openfa.exe $(TMP)
	strip $(TMP)/*
	pushd $(TMP) && zip ../openfa-win-x86_64.zip ./* && popd
	rm -rf $(TMP)

ofa-tools-win-x86_64.zip: target/x86_64-pc-windows-gnu/release/openfa.exe
	mkdir $(TMP)
	find target/x86_64-pc-windows-gnu/release/ -type f -executable -name "dump-*.exe" -exec cp \{\} $(TMP) \;
	find target/x86_64-pc-windows-gnu/release/ -type f -executable -name "show-*.exe" -exec cp \{\} $(TMP) \;
	strip $(TMP)/*
	pushd $(TMP) && zip ../ofa-tools-win-x86_64.zip ./* && popd
	rm -rf $(TMP)


target/release/openfa:
	cargo build --workspace --all-targets --release

openfa-linux-x86_64.tar.bz2: target/release/openfa
	mkdir $(TMP)
	cp target/release/openfa $(TMP)
	strip $(TMP)/*
	tar -C $(TMP) -cjvf openfa-linux-x86_64.tar.bz2 .
	rm -rf $(TMP)

ofa-tools-linux-x86_64.tar.bz2: target/release/openfa
	mkdir $(TMP)
	find target/release/ -type f -executable -name "dump-*" -exec cp \{\} $(TMP) \;
	find target/release/ -type f -executable -name "show-*" -exec cp \{\} $(TMP) \;
	strip $(TMP)/*
	tar -C $(TMP) -cjvf ofa-tools-linux-x86_64.tar.bz2 .
	rm -rf $(TMP)


release: openfa-linux-x86_64.tar.bz2 ofa-tools-linux-x86_64.tar.bz2 openfa-win-x86_64.zip ofa-tools-win-x86_64.zip
	aws s3 cp openfa-linux-x86_64.tar.bz2 s3://openfa/$(PKG_OFA_LINUX_X64)
	aws s3 cp ofa-tools-linux-x86_64.tar.bz2 s3://openfa/$(PKG_TOOLS_LINUX_X64)
	aws s3 cp openfa-win-x86_64.zip s3://openfa/$(PKG_OFA_WIN_X64)
	aws s3 cp ofa-tools-win-x86_64.zip s3://openfa/$(PKG_TOOLS_WIN_X64)
	release-cli --server-url "https://gitlab.com/" --project-id $(PROJECT_ID) --private-token $(shell cat ~/.gitlab/token) \
        create --name "Release Alpha-$(V)" --tag-name $(V) --ref $(shell git show-ref --hash --head HEAD | head -n1) \
        --assets-link "{\"name\":\"${PKG_OFA_LINUX_X64}\",\"url\":\"${OFA_BUCKET_URL}/${PKG_OFA_LINUX_X64}\"}" \
        --assets-link "{\"name\":\"${PKG_TOOLS_LINUX_X64}\",\"url\":\"${OFA_BUCKET_URL}/${PKG_TOOLS_LINUX_X64}\"}" \
        --assets-link "{\"name\":\"${PKG_OFA_WIN_X64}\",\"url\":\"${OFA_BUCKET_URL}/${PKG_OFA_WIN_X64}\"}" \
        --assets-link "{\"name\":\"${PKG_TOOLS_WIN_X64}\",\"url\":\"${OFA_BUCKET_URL}/${PKG_TOOLS_WIN_X64}\"}"
	aws s3api put-object-acl --bucket openfa --key $(PKG_OFA_LINUX_X64) --acl public-read
	aws s3api put-object-acl --bucket openfa --key $(PKG_TOOLS_LINUX_X64) --acl public-read
	aws s3api put-object-acl --bucket openfa --key $(PKG_OFA_WIN_X64) --acl public-read
	aws s3api put-object-acl --bucket openfa --key $(PKG_TOOLS_WIN_X64) --acl public-read

